use glam::{Vec3, Vec4};

use crate::rasterizer::depth_buffer::DepthBuffer;
use pic_data::{VoxelColumn, VoxelSlices};

/// Distance (map units) beyond which edge-on voxel face directions are culled.
const CULL_DIST: f32 = 42.0;
const CULL_DIST_SQ: f32 = CULL_DIST * CULL_DIST;
/// Face directions with |dot(normal, view_dir)| below this are culled at
/// distance.
const CULL_DOT_THRESHOLD: f32 = 0.15;

pub struct VoxelCollectParams<'a> {
    pub base_pos: Vec3,
    pub cos_a: f32,
    pub sin_a: f32,
    pub brightness: usize,
    pub player_pos: Vec3,
    pub view_proj: &'a glam::Mat4,
    pub screen_width: u32,
    pub screen_height: u32,
    pub is_shadow: bool,
}

pub struct VoxelSliceRef {
    pub origin: Vec3,
    pub u_vec: Vec3,
    pub v_vec: Vec3,
    pub brightness: usize,
    pub width: u16,
    pub height: u16,
    pub axis: u8,
    pub columns: *const [VoxelColumn],
    pub depth: f32,
    pub is_shadow: bool,
}

// SAFETY: VoxelSliceRef borrows data from VoxelSlices which lives
// for the duration of the frame (game) or the app lifetime (viewer).
unsafe impl Send for VoxelSliceRef {}

pub enum CollectResult {
    Behind,
    HizCulled,
    /// (slices_collected, slices_normal_culled)
    Collected(u32, u32),
}

pub fn collect_visible_slices(
    vslices: &VoxelSlices,
    params: &VoxelCollectParams,
    depth_buffer: &DepthBuffer,
    out: &mut Vec<VoxelSliceRef>,
) -> CollectResult {
    let base_x = params.base_pos.x;
    let base_y = params.base_pos.y;
    let base_z = params.base_pos.z;
    let cos_a = params.cos_a;
    let sin_a = params.sin_a;
    let hx = vslices.xpivot;
    let hy = vslices.ypivot;
    let hz = vslices.zpivot;
    let player_pos = params.player_pos;
    let view_proj = params.view_proj;
    let screen_width = params.screen_width;
    let screen_height = params.screen_height;

    // BBox occlusion test: project 8 corners of model bounding box
    let sx = vslices.xsiz as f32;
    let sy = vslices.ysiz as f32;
    let sz = vslices.zsiz as f32;

    let mut all_behind = true;
    let mut scr_min_x = f32::MAX;
    let mut scr_max_x = f32::MIN;
    let mut scr_min_y = f32::MAX;
    let mut scr_max_y = f32::MIN;
    let mut max_inv_w: f32 = 0.0;
    let half_w = screen_width as f32 * 0.5;
    let half_h = screen_height as f32 * 0.5;

    for cz in [0.0f32, sz] {
        for cy in [0.0f32, sy] {
            for cx in [0.0f32, sx] {
                let ox = cx - hx;
                let oy = cy - hy;
                let wx = base_x + ox * cos_a - oy * sin_a;
                let wy = base_y + ox * sin_a + oy * cos_a;
                let wz = base_z - (cz - hz);
                let rel = Vec3::new(wx - player_pos.x, wy - player_pos.y, wz - player_pos.z);
                let clip = *view_proj * Vec4::new(rel.x, rel.y, rel.z, 1.0);
                if clip.w > 0.0 {
                    all_behind = false;
                    let inv_w = 1.0 / clip.w;
                    max_inv_w = max_inv_w.max(inv_w);
                    let sx = (clip.x + clip.w) * half_w * inv_w;
                    let sy = (clip.w - clip.y) * half_h * inv_w;
                    scr_min_x = scr_min_x.min(sx);
                    scr_max_x = scr_max_x.max(sx);
                    scr_min_y = scr_min_y.min(sy);
                    scr_max_y = scr_max_y.max(sy);
                }
            }
        }
    }
    if all_behind {
        return CollectResult::Behind;
    }

    let scr_w = screen_width as f32;
    let scr_h = screen_height as f32;
    if scr_max_x < 0.0 || scr_min_x >= scr_w || scr_max_y < 0.0 || scr_min_y >= scr_h {
        return CollectResult::Behind;
    }

    let sx0 = scr_min_x.max(0.0) as usize;
    let sy0 = scr_min_y.max(0.0) as usize;
    let sx1 = (scr_max_x as usize).min(screen_width as usize - 1);
    let sy1 = (scr_max_y as usize).min(screen_height as usize - 1);
    if sx0 <= sx1 && sy0 <= sy1 && depth_buffer.is_occluded_hiz(sx0, sy0, sx1, sy1, max_inv_w) {
        return CollectResult::HizCulled;
    }

    let to_obj = Vec3::new(
        base_x - player_pos.x,
        base_y - player_pos.y,
        base_z - player_pos.z,
    );
    let cull_edge_on = to_obj.length_squared() > CULL_DIST_SQ;

    let axis_nx = [-cos_a, sin_a, 0.0f32];
    let axis_ny = [-sin_a, -cos_a, 0.0f32];
    let axis_nz = [0.0f32, 0.0f32, 1.0f32];

    let axis_d_vec: [Vec3; 3] = [
        Vec3::new(cos_a, sin_a, 0.0),
        Vec3::new(-sin_a, cos_a, 0.0),
        Vec3::new(0.0, 0.0, -1.0),
    ];

    let mut count = 0u32;
    let mut normal_culled = 0u32;
    for axis in 0..3 {
        let d_vec = axis_d_vec[axis];
        let (nx, ny, nz) = (axis_nx[axis], axis_ny[axis], axis_nz[axis]);

        for quad in &vslices.slices[axis] {
            let d_neg = quad.depth;
            let u0 = quad.min_u as f32;
            let v0 = quad.min_v as f32;

            let origin_neg = match axis {
                0 => {
                    let ox = d_neg - hx;
                    let oy = u0 - hy;
                    Vec3::new(
                        base_x + ox * cos_a - oy * sin_a,
                        base_y + ox * sin_a + oy * cos_a,
                        base_z + (hz - v0),
                    )
                }
                1 => {
                    let ox = u0 - hx;
                    let oy = d_neg - hy;
                    Vec3::new(
                        base_x + ox * cos_a - oy * sin_a,
                        base_y + ox * sin_a + oy * cos_a,
                        base_z + (hz - v0),
                    )
                }
                _ => {
                    let ox = u0 - hx;
                    let oy = v0 - hy;
                    Vec3::new(
                        base_x + ox * cos_a - oy * sin_a,
                        base_y + ox * sin_a + oy * cos_a,
                        base_z + (hz - d_neg),
                    )
                }
            };

            let corner = |d: f32, u: f32, v: f32| -> Vec3 {
                let (ox, oy, oz) = match axis {
                    0 => (d - hx, u - hy, -(v - hz)),
                    1 => (u - hx, d - hy, -(v - hz)),
                    _ => (u - hx, v - hy, -(d - hz)),
                };
                let rx = ox * cos_a - oy * sin_a;
                let ry = ox * sin_a + oy * cos_a;
                Vec3::new(base_x + rx, base_y + ry, base_z + oz)
            };
            let u_vec = corner(d_neg, u0 + 1.0, v0) - origin_neg;
            let v_vec = corner(d_neg, u0, v0 + 1.0) - origin_neg;

            let center =
                origin_neg + u_vec * (quad.width as f32 * 0.5) + v_vec * (quad.height as f32 * 0.5);
            let cdx = player_pos.x - center.x;
            let cdy = player_pos.y - center.y;
            let cdz = player_pos.z - center.z;
            let slice_depth = cdx * cdx + cdy * cdy + cdz * cdz;

            let to_quad_dot = -(nx * cdx + ny * cdy + nz * cdz);
            let dist = slice_depth.sqrt();

            if cull_edge_on && dist > CULL_DIST && to_quad_dot.abs() < CULL_DOT_THRESHOLD * dist {
                normal_culled += 1;
                continue;
            }

            let (columns, origin) = if to_quad_dot < 0.0 {
                (&quad.neg_columns, origin_neg)
            } else {
                (&quad.pos_columns, origin_neg + d_vec)
            };
            if columns.is_empty() {
                normal_culled += 1;
                continue;
            }

            // Per-quad frustum + hi-Z cull
            let w = quad.width as f32;
            let h = quad.height as f32;
            let p0 = origin;
            let p1 = origin + u_vec * w;
            let p2 = origin + u_vec * w + v_vec * h;
            let p3 = origin + v_vec * h;

            let mut q_all_behind = true;
            let mut q_min_x = f32::MAX;
            let mut q_max_x = f32::MIN;
            let mut q_min_y = f32::MAX;
            let mut q_max_y = f32::MIN;
            let mut q_max_iw: f32 = 0.0;

            for p in [p0, p1, p2, p3] {
                let rel = p - player_pos;
                let clip = *view_proj * Vec4::new(rel.x, rel.y, rel.z, 1.0);
                if clip.w > 0.0 {
                    q_all_behind = false;
                    let iw = 1.0 / clip.w;
                    q_max_iw = q_max_iw.max(iw);
                    let sx = (clip.x + clip.w) * half_w * iw;
                    let sy = (clip.w - clip.y) * half_h * iw;
                    q_min_x = q_min_x.min(sx);
                    q_max_x = q_max_x.max(sx);
                    q_min_y = q_min_y.min(sy);
                    q_max_y = q_max_y.max(sy);
                }
            }

            if q_all_behind {
                normal_culled += 1;
                continue;
            }

            if q_max_x < 0.0 || q_min_x >= scr_w || q_max_y < 0.0 || q_min_y >= scr_h {
                normal_culled += 1;
                continue;
            }

            let qx0 = q_min_x.max(0.0) as usize;
            let qy0 = q_min_y.max(0.0) as usize;
            let qx1 = (q_max_x as usize).min(screen_width as usize - 1);
            let qy1 = (q_max_y as usize).min(screen_height as usize - 1);
            if qx0 <= qx1
                && qy0 <= qy1
                && depth_buffer.is_occluded_hiz(qx0, qy0, qx1, qy1, q_max_iw)
            {
                normal_culled += 1;
                continue;
            }

            count += 1;
            out.push(VoxelSliceRef {
                origin,
                u_vec,
                v_vec,
                brightness: params.brightness,
                width: quad.width,
                height: quad.height,
                axis: axis as u8,
                columns: &columns[..] as *const [VoxelColumn],
                depth: slice_depth,
                is_shadow: params.is_shadow,
            });
        }
    }
    CollectResult::Collected(count, normal_culled)
}
