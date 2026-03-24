use glam::Vec2;

/// Subdivision interval for perspective correction. Exact UV recomputed every
/// N pixels; linear stepping between. Must be power of 2.
const SUBDIV_INTERVAL: u32 = 4;

#[derive(Debug, Clone)]
pub(crate) struct InterpolationState {
    // Raw perspective-space values (linear in screen space)
    current_tex: Vec2,
    current_inv_w: f32,
    tex_dx: Vec2,
    inv_w_dx: f32,
    inv_w_min: f32,
    inv_w_max: f32,
    // Cached perspective-corrected UVs and linear step between subdivisions
    cached_u: f32,
    cached_v: f32,
    du: f32,
    dv: f32,
    // Pixels until next exact recomputation
    steps_remaining: u32,
}

impl InterpolationState {
    #[inline(always)]
    fn correct_uv(tex: Vec2, inv_w: f32, inv_w_min: f32, inv_w_max: f32) -> (f32, f32) {
        let clamped = inv_w.clamp(inv_w_min, inv_w_max);
        if clamped > 0.0 {
            let w = 1.0 / clamped;
            (tex.x * w, tex.y * w)
        } else {
            (tex.x, tex.y)
        }
    }

    /// Recompute exact UVs at current position and N pixels ahead,
    /// then set up linear stepping between them.
    #[inline(always)]
    fn recompute_subdivision(&mut self) {
        let (u0, v0) = Self::correct_uv(
            self.current_tex,
            self.current_inv_w,
            self.inv_w_min,
            self.inv_w_max,
        );

        // Sample N pixels ahead
        let future_tex = self.current_tex + self.tex_dx * SUBDIV_INTERVAL as f32;
        let future_inv_w = self.current_inv_w + self.inv_w_dx * SUBDIV_INTERVAL as f32;
        let (u1, v1) = Self::correct_uv(future_tex, future_inv_w, self.inv_w_min, self.inv_w_max);

        let inv_n = 1.0 / SUBDIV_INTERVAL as f32;
        self.du = (u1 - u0) * inv_n;
        self.dv = (v1 - v0) * inv_n;
        self.cached_u = u0;
        self.cached_v = v0;
        self.steps_remaining = SUBDIV_INTERVAL;
    }

    #[inline(always)]
    pub(crate) fn get_current_uv(&self) -> (f32, f32) {
        (self.cached_u, self.cached_v)
    }

    #[inline(always)]
    pub(crate) fn step_x(&mut self) {
        // Always advance the raw perspective-space values (cheap adds)
        self.current_tex += self.tex_dx;
        self.current_inv_w += self.inv_w_dx;

        self.steps_remaining -= 1;
        if self.steps_remaining == 0 {
            // Recompute exact perspective-correct UVs (1 divide + setup)
            self.recompute_subdivision();
        } else {
            // Linear step between subdivision points (2 adds)
            self.cached_u += self.du;
            self.cached_v += self.dv;
        }
    }
}

/// Pre-computed triangle interpolation data for efficient per-pixel texture
/// coordinate calculation
#[derive(Debug, Clone)]
pub(crate) struct TriangleInterpolator {
    v0: Vec2,
    v1: Vec2,
    v2: Vec2,
    tex0: Vec2,
    tex1: Vec2,
    tex2: Vec2,
    inv_w0: f32,
    inv_w1: f32,
    inv_w2: f32,
    denom: f32,
    da_dx: f32,
    db_dx: f32,
    /// Min/max inv_w across all polygon vertices, used to clamp extrapolated
    /// depth
    inv_w_min: f32,
    inv_w_max: f32,
}

impl TriangleInterpolator {
    #[inline(always)]
    pub(crate) fn new(screen_verts: &[Vec2], tex_coords: &[Vec2], inv_w: &[f32]) -> Option<Self> {
        // Compute min/max inv_w across all polygon vertices to clamp extrapolation
        let mut inv_w_min = f32::INFINITY;
        let mut inv_w_max = f32::NEG_INFINITY;
        for &w in inv_w.iter() {
            if w < inv_w_min {
                inv_w_min = w;
            }
            if w > inv_w_max {
                inv_w_max = w;
            }
        }

        // Fast path for triangles - no need to search for best triangle
        if screen_verts.len() == 3 {
            let v0 = screen_verts[0];
            let v1 = screen_verts[1];
            let v2 = screen_verts[2];

            let denom = (v1.y - v2.y) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.y - v2.y);
            if denom.abs() < f32::EPSILON {
                return None;
            }
            let da_dx = (v1.y - v2.y) / denom;
            let db_dx = (v2.y - v0.y) / denom;

            return Some(TriangleInterpolator {
                v0,
                v1,
                v2,
                tex0: tex_coords[0],
                tex1: tex_coords[1],
                tex2: tex_coords[2],
                inv_w0: inv_w[0],
                inv_w1: inv_w[1],
                inv_w2: inv_w[2],
                denom,
                da_dx,
                db_dx,
                inv_w_min,
                inv_w_max,
            });
        }

        // For polygons with more than 3 vertices, find the best triangle
        let mut best_triangle = None;
        let mut best_area = 0.0;
        let mut best_denom = 0.0;

        for i in 1..screen_verts.len() - 1 {
            let v0 = screen_verts[0];
            let v1 = screen_verts[i];
            let v2 = screen_verts[i + 1];

            let denom = (v1.y - v2.y) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.y - v2.y);
            if denom.abs() < f32::EPSILON {
                continue;
            }

            let area = denom.abs();
            if area > best_area {
                best_area = area;
                best_triangle = Some((0, i, i + 1));
                best_denom = denom;
            }
        }

        let (i0, i1, i2) = best_triangle?;
        let v0 = screen_verts[i0];
        let v1 = screen_verts[i1];
        let v2 = screen_verts[i2];

        let denom = best_denom;

        // Pre-compute barycentric derivatives
        let da_dx = (v1.y - v2.y) / denom;
        let db_dx = (v2.y - v0.y) / denom;

        Some(TriangleInterpolator {
            v0,
            v1,
            v2,
            tex0: tex_coords[i0],
            tex1: tex_coords[i1],
            tex2: tex_coords[i2],
            inv_w0: inv_w[i0],
            inv_w1: inv_w[i1],
            inv_w2: inv_w[i2],
            denom,
            da_dx,
            db_dx,
            inv_w_min,
            inv_w_max,
        })
    }

    /// Initialize interpolation state for a scanline
    #[inline(always)]
    pub(crate) fn init_scanline(&self, start_x: f32, y: f32) -> InterpolationState {
        let p = Vec2::new(start_x, y);

        // Calculate initial barycentric coordinates
        let a = ((self.v1.y - self.v2.y) * (p.x - self.v2.x)
            + (self.v2.x - self.v1.x) * (p.y - self.v2.y))
            / self.denom;
        let b = ((self.v2.y - self.v0.y) * (p.x - self.v2.x)
            + (self.v0.x - self.v2.x) * (p.y - self.v2.y))
            / self.denom;
        let c = 1.0 - a - b;

        // Calculate initial interpolated values
        let interp_tex = self.tex0 * a + self.tex1 * b + self.tex2 * c;
        let interp_inv_w = self.inv_w0 * a + self.inv_w1 * b + self.inv_w2 * c;

        // Calculate per-pixel increments for X direction
        let tex_dx = self.tex0 * self.da_dx
            + self.tex1 * self.db_dx
            + self.tex2 * (-self.da_dx - self.db_dx);
        let inv_w_dx = self.inv_w0 * self.da_dx
            + self.inv_w1 * self.db_dx
            + self.inv_w2 * (-self.da_dx - self.db_dx);

        let mut state = InterpolationState {
            current_tex: interp_tex,
            current_inv_w: interp_inv_w,
            tex_dx,
            inv_w_dx,
            inv_w_min: self.inv_w_min,
            inv_w_max: self.inv_w_max,
            cached_u: 0.0,
            cached_v: 0.0,
            du: 0.0,
            dv: 0.0,
            steps_remaining: 0,
        };
        // Compute initial subdivision
        state.recompute_subdivision();
        state
    }
}
