//! Original voxel collection and software rasterization paths.
#[cfg(feature = "hprof")]
use coarse_prof::profile;
use glam::{Mat4, Vec3};
use pic_data::PalLit;
use pic_data::voxel::kvx::VoxelModel;
use pic_data::voxel::slices::VoxelSlices;
use render_common::{BufferSize, PixelTarget};
use software3d::rasterizer::Rasterizer;
use software3d::voxel::collect::{VoxelCollectParams, VoxelSliceRef, collect_visible_slices};
use std::{fmt::Write as _, mem::take, time::Instant};

pub(crate) const W: u32 = 640;
pub(crate) const H: u32 = 480;
#[expect(
    clippy::cast_precision_loss,
    reason = "the fixed 480-pixel height is exactly representable in f32"
)]
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

    #[expect(
        clippy::cast_precision_loss,
        reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
    )]
    fn view_proj(_pos: Vec3, fwd: Vec3, _up: Vec3) -> Mat4 {
        // Eye at origin — collection pipeline handles camera translation
        let view = Mat4::look_to_rh(Vec3::ZERO, fwd, Vec3::Z);
        let vfov = 2.0 * (H as f32 * 0.5 / FOV_SCALE).atan();
        let proj = Mat4::perspective_rh(vfov, W as f32 / H as f32, 0.1, 1000.0);
        proj * view
    }
}

pub(crate) struct Scene {
    /// Original software-renderer resolution, independent of window pixels.
    frame: Vec<u32>,
    depth: Vec<f32>,
    // Always empty between renders: only the allocation survives borrowed slices.
    slice_scratch: Vec<VoxelSliceRef<'static>>,
    dirty: bool,
    title_slice: Option<(usize, usize, usize)>,
    model: VoxelModel,
    slices: VoxelSlices,
    palette: [(u8, u8, u8); 256],
    pal_lit: PalLit<u32>,
    use_slices: bool,
    wireframe: u8,      // 0=off, 1=all quads, 2=collected only
    single_slice: bool, // view one slice at a time
    slice_index: usize, // current slice index on the selected axis
    rasterizer: Rasterizer,
    camera: Camera,
    background: u32,
    title: String,
    frames_since_print: u32,
    last_print: Instant,
    frame_time_min: f32,
    frame_time_max: f32,
    last_frame: Instant,
    #[cfg(feature = "hprof")]
    frame_count: u32,
}

impl Scene {
    #[expect(
        clippy::cast_precision_loss,
        reason = "voxel dimensions use floating point camera coordinates"
    )]
    pub(crate) fn new(
        model: VoxelModel,
        slices: VoxelSlices,
        palette: &[(u8, u8, u8); 256],
        pal_lit: PalLit<u32>,
        use_slices: bool,
    ) -> Self {
        let focus = Vec3::new(
            model.xsiz as f32 * 0.5 - slices.xpivot,
            model.ysiz as f32 * 0.5 - slices.ypivot,
            slices.zpivot - model.zsiz as f32 * 0.5,
        );
        Self {
            frame: vec![0; (W * H) as usize],
            depth: Vec::new(),
            slice_scratch: Vec::new(),
            dirty: true,
            title_slice: None,
            model,
            slices,
            palette: *palette,
            pal_lit,
            use_slices,
            wireframe: 0,
            single_slice: false,
            slice_index: 0,
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
            background: 0xFF1E_1E23,
            title: String::new(),
            frames_since_print: 0,
            last_print: Instant::now(),
            frame_time_min: f32::MAX,
            frame_time_max: 0.0,
            last_frame: Instant::now(),
            #[cfg(feature = "hprof")]
            frame_count: 0,
        }
    }

    pub(crate) fn frame(&self) -> Vec<u8> {
        self.frame
            .iter()
            .flat_map(|pixel| pixel.to_le_bytes())
            .collect()
    }

    pub(crate) fn animating(&self) -> bool {
        self.camera.auto_rotate
    }

    pub(crate) fn needs_frame(&self) -> bool {
        self.dirty || self.animating()
    }

    pub(crate) fn title(&self) -> &str {
        if self.single_slice {
            &self.title
        } else if self.use_slices {
            "Voxel Viewer [SLICES]"
        } else {
            "Voxel Viewer [DIRECT]"
        }
    }

    pub(crate) fn background(&mut self, colour: [u8; 4]) {
        let [r, g, b, a] = colour;
        let background = u32::from_be_bytes([a, r, g, b]);
        self.dirty |= self.background != background;
        self.background = background;
    }

    pub(crate) fn key(&mut self, key: &str) {
        match key {
            "left" => self.camera.pan_x -= 1.0,
            "right" => self.camera.pan_x += 1.0,
            "up" => self.camera.pan_y += 1.0,
            "down" => self.camera.pan_y -= 1.0,
            "=" | "+" => self.camera.zoom = (self.camera.zoom - 0.1).max(0.1),
            "-" => self.camera.zoom += 0.1,
            "r" => self.camera.auto_rotate = !self.camera.auto_rotate,
            "s" => self.use_slices = !self.use_slices,
            "w" => self.wireframe = (self.wireframe + 1) % 3,
            "d" => {
                self.single_slice = !self.single_slice;
                self.slice_index = 0;
            }
            "pause-rotate" => self.camera.auto_rotate = false,
            _ => return,
        }
        self.dirty = true;
    }

    pub(crate) fn drag(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0) {
            return;
        }
        self.dirty = true;
        self.camera.yaw -= dx * 0.01;
        self.camera.pitch = (self.camera.pitch + dy * 0.01).clamp(-1.5, 1.5);
    }

    pub(crate) fn scroll(&mut self, scroll: f32) {
        if !scroll.is_finite() || scroll == 0.0 {
            return;
        }
        self.dirty = true;
        if self.single_slice {
            if scroll > 0.0 && self.slice_index > 0 {
                self.slice_index -= 1;
            } else if scroll < 0.0 {
                self.slice_index += 1;
            }
        } else {
            self.camera.zoom = (self.camera.zoom - scroll * 0.1).max(0.1);
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
    )]
    #[expect(
        clippy::too_many_lines,
        reason = "keep the complete dispatch or geometry pass together without changing its established ordering"
    )]
    pub(crate) fn render(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("render");

        // Take the compose buffer so `self.rasterizer`/`self.pal_lit` can be
        // borrowed disjointly during render; restored before present.
        let mut owned = take(&mut self.frame);
        let frame: &mut [u32] = &mut owned;
        // Clear to dark grey (0xAARRGGBB).
        frame.fill(self.background);

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
            self.depth.resize((W * H) as usize, f32::MAX);
            self.depth.fill(f32::MAX);
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
                            draw_rect(frame, &mut self.depth, sx, sy, d, sw, sh, r, g, b);
                            rendered += 1;
                        }
                    }
                }
            }
        } else {
            #[cfg(feature = "hprof")]
            profile!("slice_render");
            let view_proj = Camera::view_proj(cam_pos, fwd, up);
            let half_w = W as f32 * 0.5;
            let half_h = H as f32 * 0.5;
            let identity_map: [usize; 256] = std::array::from_fn(|i| i);
            let colourmaps: [&[usize]; 48] = [&identity_map; 48];
            self.rasterizer.depth_buffer_mut().reset();

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
            let mut voxel_slices = take(&mut self.slice_scratch);
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
                    let quad = &axis_quads[self.slice_index];
                    // Render both sides of this quad
                    let hx = self.slices.xpivot;
                    let hy = self.slices.ypivot;
                    let hz = self.slices.zpivot;
                    for &(columns, d) in &[
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
                        voxel_slices.push(VoxelSliceRef {
                            origin,
                            u_vec,
                            v_vec,
                            brightness: 15,
                            width: quad.width,
                            height: quad.height,
                            axis: dominant_axis as u8,
                            columns: &columns[..],
                            depth: 0.0,
                            is_shadow: false,
                        });
                    }
                }
                let axis_name = ["X", "Y", "Z"][dominant_axis];
                let title_slice = (dominant_axis, self.slice_index, axis_count);
                if self.title_slice != Some(title_slice) {
                    self.title.clear();
                    let _ = write!(
                        self.title,
                        "Voxel Viewer [SLICE {} {}/{}]",
                        axis_name,
                        self.slice_index + 1,
                        axis_count
                    );
                    self.title_slice = Some(title_slice);
                }
            } else {
                collect_visible_slices(
                    &self.slices,
                    &params,
                    self.rasterizer.depth_buffer(),
                    &mut voxel_slices,
                );
            }

            // Sort front-to-back for optimal depth rejection
            voxel_slices.sort_unstable_by(|a, b| a.depth.total_cmp(&b.depth));

            {
                let size = BufferSize::new(W as usize, H as usize);
                let mut target = PixelTarget::new(&mut *frame, size, W as usize, &self.pal_lit, 0);
                for vq in &voxel_slices {
                    let columns = vq.columns;
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
                        &mut target,
                        W as usize,
                    );
                    rendered += 1;
                }
            }

            // Wireframe overlay: w=1 all quads, w=2 collected (backface-culled) only
            if self.wireframe > 0 {
                let axis_colors: [u32; 3] = [0xFF0000FF, 0xFF00FF00, 0xFFFF0000];
                if self.wireframe == 2 {
                    // Draw outlines of collected slices (same as rendered), coloured by axis
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
                        let proj = corners_3d.map(|p| {
                            let rel = p - cam_pos;
                            let c = view_proj * glam::Vec4::new(rel.x, rel.y, rel.z, 1.0);
                            if c.w > 0.0 {
                                let iw = 1.0 / c.w;
                                Some(((c.x + c.w) * half_w * iw, (c.w - c.y) * half_h * iw))
                            } else {
                                None
                            }
                        });
                        for i in 0..4 {
                            if let (Some(a), Some(b)) = (proj[i], proj[(i + 1) % 4]) {
                                draw_line(frame, W as i32, H as i32, a.0, a.1, b.0, b.1, color);
                            }
                        }
                    }
                } else {
                    // Draw all non-empty quads, coloured per axis
                    for (axis, color) in axis_colors.iter().enumerate() {
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
                                let proj = corners_3d.map(|p| {
                                    let rel = p - cam_pos;
                                    let c = view_proj * glam::Vec4::new(rel.x, rel.y, rel.z, 1.0);
                                    if c.w > 0.0 {
                                        let iw = 1.0 / c.w;
                                        Some(((c.x + c.w) * half_w * iw, (c.w - c.y) * half_h * iw))
                                    } else {
                                        None
                                    }
                                });
                                for i in 0..4 {
                                    if let (Some(a), Some(b)) = (proj[i], proj[(i + 1) % 4]) {
                                        draw_line(
                                            frame, W as i32, H as i32, a.0, a.1, b.0, b.1, *color,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            self.slice_scratch = recycle_slices(voxel_slices);
        }

        self.frame = owned;
        self.dirty = false;

        #[cfg(feature = "hprof")]
        {
            self.frame_count += 1;
            if self.frame_count % 120 == 0 {
                coarse_prof::write(&mut std::io::stdout()).unwrap();
            }
        }
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
            log::info!(
                "{mode}: {rendered} rendered | fps: {fps:.0} | {avg_ms:.2}ms avg, {:.2}ms min, {:.2}ms max",
                self.frame_time_min,
                self.frame_time_max,
            );
            self.frames_since_print = 0;
            self.frame_time_min = f32::MAX;
            self.frame_time_max = 0.0;
            self.last_print = Instant::now();
        }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
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

#[expect(
    clippy::cast_possible_truncation,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
fn draw_rect(
    frame: &mut [u32],
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
    let colour = 0xFF00_0000 | ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
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
                frame[idx] = colour;
            }
        }
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
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

// Drop every borrowed reference before widening the empty vector's lifetime.
// Same-layout in-place collection retains the backing allocation without unsafe
// code or references from Scene into its own model.
fn recycle_slices(mut slices: Vec<VoxelSliceRef<'_>>) -> Vec<VoxelSliceRef<'static>> {
    slices.clear();
    slices.into_iter().map(|_| unreachable!()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pic_data::{ByteOrder, PALETTE_LEN, WadPalette};

    #[test]
    fn paused_frames_and_recycled_scratch_preserve_pixels() {
        let model = VoxelModel {
            xsiz: 4,
            ysiz: 4,
            zsiz: 4,
            xpivot: 2.,
            ypivot: 2.,
            zpivot: 4.,
            grid: vec![1; 64],
            palette: None,
        };
        let slices = pic_data::voxel::slices::generate(&model);
        let pal = PalLit::new(
            &[WadPalette([0xFFCC8844; 256]); PALETTE_LEN],
            ByteOrder::Argb,
        );
        let mut scene = Scene::new(model, slices, &[(204, 136, 68); 256], pal, true);
        scene.key("pause-rotate");
        scene.render();
        assert!(!scene.needs_frame());
        assert!(scene.frame.contains(&0xFFCC8844));
        let first = scene.frame.clone();
        let allocation = scene.slice_scratch.as_ptr();
        assert!(scene.slice_scratch.capacity() > 0);
        scene.render();
        assert_eq!(scene.frame, first);
        assert_eq!(scene.slice_scratch.as_ptr(), allocation);
        scene.key("d");
        assert!(scene.needs_frame());
        scene.render();
        assert!(scene.frame.contains(&0xFFCC8844));
        scene.key("s");
        scene.render();
        let direct = scene.frame.clone();
        let depth = scene.depth.as_ptr();
        scene.render();
        assert_eq!(scene.frame, direct);
        assert_eq!(scene.depth.as_ptr(), depth);
    }
}
