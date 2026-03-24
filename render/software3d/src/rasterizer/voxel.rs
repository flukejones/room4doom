use pic_data::VoxelColumn;
use render_common::{FUZZ_TABLE, fuzz_darken};

use super::Rasterizer;

#[inline(always)]
fn pdiv(c: glam::Vec4, hw: f32, hh: f32) -> Option<(f32, f32, f32)> {
    if c.w <= 0.0 {
        return None;
    }
    let iw = 1.0 / c.w;
    Some(((c.x + c.w) * hw * iw, (c.w - c.y) * hh * iw, iw))
}

struct VoxelSetup {
    h_f32: f32,
    w_i32: i32,
    half_w: f32,
    half_h: f32,
    clip_origin: glam::Vec4,
    clip_du: glam::Vec4,
    clip_dv: glam::Vec4,
    clip_duv: glam::Vec4,
}

impl Rasterizer {
    fn voxel_setup(
        &self,
        origin: glam::Vec3,
        u_vec: glam::Vec3,
        v_vec: glam::Vec3,
        tex_width: u16,
        tex_height: u16,
        view_proj: &glam::Mat4,
        camera_pos: glam::Vec3,
    ) -> Option<VoxelSetup> {
        let w_f32 = self.width as f32;
        let h_f32 = self.view_height as f32;
        let half_w = w_f32 * 0.5;
        let half_h = h_f32 * 0.5;

        let m = *view_proj;
        let mc0 = m.col(0);
        let mc1 = m.col(1);
        let mc2 = m.col(2);
        let mc3 = m.col(3);

        let rel_origin = origin - camera_pos;
        let clip_origin = mc0 * rel_origin.x + mc1 * rel_origin.y + mc2 * rel_origin.z + mc3;
        let clip_du = mc0 * u_vec.x + mc1 * u_vec.y + mc2 * u_vec.z;
        let clip_dv = mc0 * v_vec.x + mc1 * v_vec.y + mc2 * v_vec.z;

        // Hi-Z rejection using 4 clip-space corners
        let tw = tex_width as f32;
        let th = tex_height as f32;
        let corners = [
            clip_origin,
            clip_origin + clip_du * tw,
            clip_origin + clip_du * tw + clip_dv * th,
            clip_origin + clip_dv * th,
        ];
        let mut all_behind = true;
        let mut scr_min_x = f32::MAX;
        let mut scr_max_x = f32::MIN;
        let mut scr_min_y = f32::MAX;
        let mut scr_max_y = f32::MIN;
        let mut max_iw: f32 = 0.0;
        for c in &corners {
            if c.w > 0.0 {
                all_behind = false;
                let iw = 1.0 / c.w;
                max_iw = max_iw.max(iw);
                let sx = (c.x + c.w) * half_w * iw;
                let sy = (c.w - c.y) * half_h * iw;
                scr_min_x = scr_min_x.min(sx);
                scr_max_x = scr_max_x.max(sx);
                scr_min_y = scr_min_y.min(sy);
                scr_max_y = scr_max_y.max(sy);
            }
        }
        if all_behind {
            return None;
        }
        if scr_max_x < 0.0 || scr_min_x >= w_f32 || scr_max_y < 0.0 || scr_min_y >= h_f32 {
            return None;
        }
        let sx0 = scr_min_x.max(0.0) as usize;
        let sy0 = scr_min_y.max(0.0) as usize;
        let sx1 = (scr_max_x as usize).min(self.width as usize - 1);
        let sy1 = (scr_max_y as usize).min(self.view_height as usize - 1);
        if sx0 <= sx1
            && sy0 <= sy1
            && self
                .depth_buffer
                .is_occluded_hiz(sx0, sy0, sx1, sy1, max_iw)
        {
            return None;
        }

        Some(VoxelSetup {
            h_f32,
            w_i32: self.width as i32,
            half_w,
            half_h,
            clip_origin,
            clip_du,
            clip_dv,
            clip_duv: clip_du + clip_dv,
        })
    }

    /// Render voxel slice by iterating occupied texels with clip-space
    /// incremental projection, corner caching, and scanline parallelogram fill.
    pub fn rasterize_voxel_texels(
        &mut self,
        origin: glam::Vec3,
        u_vec: glam::Vec3,
        v_vec: glam::Vec3,
        columns: &[VoxelColumn],
        tex_width: u16,
        tex_height: u16,
        view_proj: &glam::Mat4,
        camera_pos: glam::Vec3,
        colourmaps: &[&[usize]],
        palette: &[u32],
        buffer: &mut [u32],
        pitch: usize,
    ) {
        let s = match self.voxel_setup(
            origin, u_vec, v_vec, tex_width, tex_height, view_proj, camera_pos,
        ) {
            Some(s) => s,
            None => return,
        };

        let mut col_idx = 0usize;
        while col_idx < columns.len() {
            let col = &columns[col_idx];
            if col.spans.is_empty() {
                col_idx += col.skip_cols as usize;
                continue;
            }
            let u = col_idx as f32;
            let clip_col = s.clip_origin + s.clip_du * u;

            for span in &col.spans {
                let v0 = span.start as f32;
                let mut c00 = clip_col + s.clip_dv * v0;
                let mut cached_top: Option<((f32, f32, f32), (f32, f32, f32))> = None;

                for &color in &span.pixels {
                    let (s0, s1) = if let Some(top) = cached_top {
                        top
                    } else {
                        let c10 = c00 + s.clip_du;
                        match (pdiv(c00, s.half_w, s.half_h), pdiv(c10, s.half_w, s.half_h)) {
                            (Some(a), Some(b)) => (a, b),
                            _ => {
                                c00 += s.clip_dv;
                                cached_top = None;
                                continue;
                            }
                        }
                    };

                    let c01 = c00 + s.clip_dv;
                    let c11 = c00 + s.clip_duv;
                    let bot = match (pdiv(c01, s.half_w, s.half_h), pdiv(c11, s.half_w, s.half_h)) {
                        (Some(a), Some(b)) => (a, b),
                        _ => {
                            c00 += s.clip_dv;
                            cached_top = None;
                            continue;
                        }
                    };
                    let (s3, s2) = bot;
                    cached_top = Some((s3, s2));

                    let min_y = s0.1.min(s1.1).min(s2.1).min(s3.1).max(0.0) as i32;
                    let max_y = s0.1.max(s1.1).max(s2.1).max(s3.1).min(s.h_f32 - 1.0) as i32;

                    if min_y <= max_y {
                        let avg_iw = (s0.2 + s1.2 + s2.2 + s3.2) * 0.25;

                        let min_x = s0.0.min(s1.0).min(s2.0).min(s3.0).max(0.0) as usize;
                        let max_x = (s0.0.max(s1.0).max(s2.0).max(s3.0) as usize)
                            .min(self.width as usize - 1);
                        let min_yu = min_y as usize;
                        let max_yu = max_y as usize;
                        if min_x <= max_x
                            && self
                                .depth_buffer
                                .is_occluded_hiz(min_x, min_yu, max_x, max_yu, avg_iw)
                        {
                            c00 += s.clip_dv;
                            continue;
                        }

                        let band = (avg_iw * super::LIGHT_SCALE).min(47.0) as usize;
                        let colourmap = colourmaps[band];
                        let lit_index = colourmap[color as usize];
                        let pixel_color = palette[lit_index];

                        if min_y == max_y {
                            let min_x = min_x as i32;
                            let max_x = max_x as i32;
                            let row = min_y as usize * pitch;
                            for px in min_x..=max_x {
                                let ux = px as usize;
                                if avg_iw
                                    > self.depth_buffer.peek_depth_unchecked(ux, min_y as usize)
                                {
                                    buffer[row + ux] = pixel_color;
                                    self.depth_buffer.set_depth_update_hiz(
                                        ux,
                                        min_y as usize,
                                        avg_iw,
                                    );
                                }
                            }
                        } else {
                            let pts = [(s0.0, s0.1), (s1.0, s1.1), (s2.0, s2.1), (s3.0, s3.1)];
                            let mut eslopes = [(0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32); 4];
                            let mut edge_count = 0u8;
                            for i in 0..4 {
                                let (x0, y0) = pts[i];
                                let (x1, y1) = pts[(i + 1) & 3];
                                let dy = y1 - y0;
                                if dy.abs() > 1e-6 {
                                    let (y_lo, y_hi) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
                                    eslopes[edge_count as usize] =
                                        (x0, y0, (x1 - x0) / dy, y_lo, y_hi);
                                    edge_count += 1;
                                }
                            }

                            for py in min_y..=max_y {
                                let fy = py as f32 + 0.5;
                                let mut scan_min = f32::MAX;
                                let mut scan_max = f32::MIN;

                                for i in 0..edge_count as usize {
                                    let (x0, y0, dx_dy, y_lo, y_hi) = eslopes[i];
                                    if fy < y_lo || fy > y_hi {
                                        continue;
                                    }
                                    let ix = x0 + (fy - y0) * dx_dy;
                                    scan_min = scan_min.min(ix);
                                    scan_max = scan_max.max(ix);
                                }

                                let px_min = scan_min.max(0.0) as i32;
                                let px_max = (scan_max as i32).min(s.w_i32 - 1);
                                let row = py as usize * pitch;
                                for px in px_min..=px_max {
                                    let ux = px as usize;
                                    if avg_iw
                                        > self.depth_buffer.peek_depth_unchecked(ux, py as usize)
                                    {
                                        buffer[row + ux] = pixel_color;
                                        self.depth_buffer.set_depth_update_hiz(
                                            ux,
                                            py as usize,
                                            avg_iw,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    c00 += s.clip_dv;
                }
            }
            col_idx += 1;
        }
    }

    /// Fuzz variant of voxel rendering — reads existing framebuffer pixels at
    /// Y-offset and darkens them, producing the spectre shimmer effect.
    pub fn rasterize_voxel_fuzz(
        &mut self,
        origin: glam::Vec3,
        u_vec: glam::Vec3,
        v_vec: glam::Vec3,
        columns: &[VoxelColumn],
        tex_width: u16,
        tex_height: u16,
        view_proj: &glam::Mat4,
        camera_pos: glam::Vec3,
        buffer: &mut [u32],
        pitch: usize,
        fuzz_pos: &mut usize,
    ) {
        let s = match self.voxel_setup(
            origin, u_vec, v_vec, tex_width, tex_height, view_proj, camera_pos,
        ) {
            Some(s) => s,
            None => return,
        };
        let h_clamp = s.h_f32 as i32 - 1;

        let mut col_idx = 0usize;
        while col_idx < columns.len() {
            let col = &columns[col_idx];
            if col.spans.is_empty() {
                col_idx += col.skip_cols as usize;
                continue;
            }
            let u = col_idx as f32;
            let clip_col = s.clip_origin + s.clip_du * u;

            for span in &col.spans {
                let v0 = span.start as f32;
                let mut c00 = clip_col + s.clip_dv * v0;
                let mut cached_top: Option<((f32, f32, f32), (f32, f32, f32))> = None;

                for &_color in &span.pixels {
                    let (s0, s1) = if let Some(top) = cached_top {
                        top
                    } else {
                        let c10 = c00 + s.clip_du;
                        match (pdiv(c00, s.half_w, s.half_h), pdiv(c10, s.half_w, s.half_h)) {
                            (Some(a), Some(b)) => (a, b),
                            _ => {
                                c00 += s.clip_dv;
                                cached_top = None;
                                continue;
                            }
                        }
                    };

                    let c01 = c00 + s.clip_dv;
                    let c11 = c00 + s.clip_duv;
                    let bot = match (pdiv(c01, s.half_w, s.half_h), pdiv(c11, s.half_w, s.half_h)) {
                        (Some(a), Some(b)) => (a, b),
                        _ => {
                            c00 += s.clip_dv;
                            cached_top = None;
                            continue;
                        }
                    };
                    let (s3, s2) = bot;
                    cached_top = Some((s3, s2));

                    let min_y = s0.1.min(s1.1).min(s2.1).min(s3.1).max(0.0) as i32;
                    let max_y = s0.1.max(s1.1).max(s2.1).max(s3.1).min(s.h_f32 - 1.0) as i32;

                    if min_y <= max_y {
                        let avg_iw = (s0.2 + s1.2 + s2.2 + s3.2) * 0.25;

                        let min_x = s0.0.min(s1.0).min(s2.0).min(s3.0).max(0.0) as usize;
                        let max_x = (s0.0.max(s1.0).max(s2.0).max(s3.0) as usize)
                            .min(self.width as usize - 1);
                        let min_yu = min_y as usize;
                        let max_yu = max_y as usize;
                        if min_x <= max_x
                            && self
                                .depth_buffer
                                .is_occluded_hiz(min_x, min_yu, max_x, max_yu, avg_iw)
                        {
                            c00 += s.clip_dv;
                            continue;
                        }

                        if min_y == max_y {
                            let min_x = min_x as i32;
                            let max_x = max_x as i32;
                            let row = min_y as usize * pitch;
                            for px in min_x..=max_x {
                                let ux = px as usize;
                                if avg_iw
                                    > self.depth_buffer.peek_depth_unchecked(ux, min_y as usize)
                                {
                                    let offset = FUZZ_TABLE[*fuzz_pos % FUZZ_TABLE.len()];
                                    let src_y = (min_y + offset).clamp(0, h_clamp) as usize;
                                    buffer[row + ux] = fuzz_darken(buffer[src_y * pitch + ux]);
                                    *fuzz_pos += 1;
                                    self.depth_buffer.set_depth_update_hiz(
                                        ux,
                                        min_y as usize,
                                        avg_iw,
                                    );
                                }
                            }
                        } else {
                            let pts = [(s0.0, s0.1), (s1.0, s1.1), (s2.0, s2.1), (s3.0, s3.1)];
                            let mut eslopes = [(0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32); 4];
                            let mut edge_count = 0u8;
                            for i in 0..4 {
                                let (x0, y0) = pts[i];
                                let (x1, y1) = pts[(i + 1) & 3];
                                let dy = y1 - y0;
                                if dy.abs() > 1e-6 {
                                    let (y_lo, y_hi) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
                                    eslopes[edge_count as usize] =
                                        (x0, y0, (x1 - x0) / dy, y_lo, y_hi);
                                    edge_count += 1;
                                }
                            }

                            for py in min_y..=max_y {
                                let fy = py as f32 + 0.5;
                                let mut scan_min = f32::MAX;
                                let mut scan_max = f32::MIN;

                                for i in 0..edge_count as usize {
                                    let (x0, y0, dx_dy, y_lo, y_hi) = eslopes[i];
                                    if fy < y_lo || fy > y_hi {
                                        continue;
                                    }
                                    let ix = x0 + (fy - y0) * dx_dy;
                                    scan_min = scan_min.min(ix);
                                    scan_max = scan_max.max(ix);
                                }

                                let px_min = scan_min.max(0.0) as i32;
                                let px_max = (scan_max as i32).min(s.w_i32 - 1);
                                let row = py as usize * pitch;
                                for px in px_min..=px_max {
                                    let ux = px as usize;
                                    if avg_iw
                                        > self.depth_buffer.peek_depth_unchecked(ux, py as usize)
                                    {
                                        let offset = FUZZ_TABLE[*fuzz_pos % FUZZ_TABLE.len()];
                                        let src_y = (py + offset).clamp(0, h_clamp) as usize;
                                        buffer[row + ux] = fuzz_darken(buffer[src_y * pitch + ux]);
                                        *fuzz_pos += 1;
                                        self.depth_buffer.set_depth_update_hiz(
                                            ux,
                                            py as usize,
                                            avg_iw,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    c00 += s.clip_dv;
                }
            }
            col_idx += 1;
        }
    }
}
