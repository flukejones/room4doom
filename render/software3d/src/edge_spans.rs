#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::{PicData, SurfacePolygon};
use glam::Vec2;
use render_trait::DrawBuffer;

use crate::depth_buffer::DepthBuffer;
use crate::render::{TextureSampler, TriangleInterpolator, write_pixel};

const SURF_NONE: usize = usize::MAX;
const SPAN_END: usize = usize::MAX;
const EDGE_NONE: usize = usize::MAX;
/// Minimum depth for real geometry (must exceed SKY_DEPTH).
const MIN_GEOMETRY_DEPTH: f32 = 1.0e-6;
const LIGHT_MIN_Z: f32 = 0.001;
const LIGHT_MAX_Z: f32 = 0.055;
const LIGHT_RANGE: f32 = 1.0 / (LIGHT_MAX_Z - LIGHT_MIN_Z);
const LIGHT_SCALE: f32 = LIGHT_RANGE * 8.0 * 16.0;
/// Left sentinel edge index (always index 0 after reset).
const AET_HEAD: usize = 0;
/// Right sentinel edge index (always index 1 after reset).
const AET_TAIL: usize = 1;

/// An edge in screen space, tracking one side of a polygon across scanlines.
/// Doubly-linked into the active edge table (AET) via `prev`/`next`.
struct Edge {
    /// Current screen-space x position (sub-pixel precision).
    x: f32,
    /// X increment per scanline.
    x_step: f32,
    /// Surface indices: [0] = trailing (surface leaving), [1] = leading
    /// (surface entering). `SURF_NONE` means no surface on that side.
    surfs: [usize; 2],
    /// 1/w at current scanline (for depth interpolation along the edge).
    inv_w: f32,
    /// 1/w increment per scanline.
    inv_w_step: f32,
    /// Previous edge in AET doubly-linked list (`EDGE_NONE` = not in AET).
    prev: usize,
    /// Next edge in AET doubly-linked list (`EDGE_NONE` = not in AET).
    next: usize,
    /// Next edge in the per-scanline new-edge chain (`EDGE_NONE` = end).
    next_new: usize,
    /// Next edge in the per-scanline removal chain (`EDGE_NONE` = end).
    next_remove: usize,
}

/// A surface registered from a visible polygon, participating in the surface
/// stack for visibility determination.
struct SpanSurface {
    /// BSP traversal order key (lower = closer to camera).
    key: usize,
    /// Index into `polygon_data[]`.
    polygon_idx: usize,
    /// Last x position where this surface was the frontmost.
    last_u: i32,
    /// Head of the span linked list (index into `spans[]`).
    span_head: usize,
    /// Edge pair counter: 0 = not on stack, 1 = active.
    spanstate: i32,
    /// 1/w at `last_u` (for depth interpolation across the span).
    last_inv_w: f32,
}

/// A horizontal span of pixels belonging to one surface on one scanline.
pub struct Span {
    /// Start x pixel (inclusive).
    x_start: usize,
    /// End x pixel (exclusive).
    x_end: usize,
    /// Scanline y.
    y: usize,
    /// 1/w at x_start.
    inv_w_start: f32,
    /// 1/w at x_end.
    inv_w_end: f32,
    /// Next span for the same surface (linked list).
    next: usize,
}

/// Pre-computed per-polygon data needed for span drawing.
pub struct PolygonRenderData {
    /// Pointer to the source polygon (valid for the frame lifetime).
    polygon: *const SurfacePolygon,
    /// Triangle interpolator for perspective-correct texture coordinates.
    interpolator: TriangleInterpolator,
    /// Sector light level (shifted).
    brightness: usize,
    /// Whether this surface is sky (depth-only, no pixel colour).
    is_sky: bool,
}

/// Per-frame statistics for the edge-span system.
#[derive(Default)]
pub struct EdgeSpanStats {
    pub edges_emitted: usize,
    pub surfaces_emitted: usize,
    pub spans_generated: usize,
    pub max_active_edges: usize,
    pub max_stack_depth: usize,
}

impl EdgeSpanStats {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

impl std::fmt::Display for EdgeSpanStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "edges: {} | surfs: {} | spans: {} | max_active: {} | max_stack: {}",
            self.edges_emitted,
            self.surfaces_emitted,
            self.spans_generated,
            self.max_active_edges,
            self.max_stack_depth,
        )
    }
}

/// Edge-based span generation state. Manages all per-frame edge, surface, and
/// span data for Quake-style rasterisation.
///
/// Storage: 3 object pools (`edges`, `surfaces`, `spans`) + 1 polygon data
/// pool + 2 per-scanline chain head arrays (`new_edges`, `remove_edges`).
/// Active edges form an intrusive doubly-linked list through `Edge::prev`/
/// `next`, bounded by sentinels at indices 0 (x=-1) and 1 (x=f32::MAX).
pub struct EdgeSpanState {
    /// Edge pool. Indices 0 and 1 are sentinels; real edges start at index 2.
    edges: Vec<Edge>,
    /// Surface pool.
    surfaces: Vec<SpanSurface>,
    /// Span pool.
    spans: Vec<Span>,
    /// Per-polygon render data pool.
    pub polygon_data: Vec<PolygonRenderData>,
    /// Per-scanline new-edge chain heads (index into `edges[]`). Each entry
    /// is the head of a singly-linked list via `Edge::next_new`.
    new_edges: Vec<usize>,
    /// Per-scanline removal chain heads (index into `edges[]`). Each entry
    /// is the head of a singly-linked list via `Edge::next_remove`.
    remove_edges: Vec<usize>,
    /// Current number of real (non-sentinel) edges in the AET.
    active_count: usize,
    /// Surface stack for visibility: indices into `surfaces[]`, sorted by key
    /// (smallest = closest).
    surface_stack: Vec<usize>,
    /// Monotonically increasing BSP traversal key.
    next_key: usize,
    /// Screen dimensions.
    width: usize,
    height: usize,
    /// Maximum screen x as f32 (width - 1), cached to avoid recomputation.
    w_max: f32,
    /// Per-frame statistics.
    pub stats: EdgeSpanStats,
}

// SAFETY: Raw pointers in PolygonRenderData point to SurfacePolygon data owned
// by BSP3D, which outlives the frame. Single-threaded access only.
unsafe impl Send for EdgeSpanState {}
unsafe impl Sync for EdgeSpanState {}

impl EdgeSpanState {
    /// Create a new edge-span state for the given screen dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        let w = width as usize;
        let h = height as usize;
        Self {
            edges: Vec::with_capacity(4096),
            surfaces: Vec::with_capacity(512),
            spans: Vec::with_capacity(8192),
            polygon_data: Vec::with_capacity(512),
            new_edges: vec![EDGE_NONE; h],
            remove_edges: vec![EDGE_NONE; h],
            active_count: 0,
            surface_stack: Vec::with_capacity(16),
            next_key: 0,
            width: w,
            height: h,
            w_max: (w - 1) as f32,
            stats: EdgeSpanStats::default(),
        }
    }

    /// Reset all per-frame state. Retains allocated memory. Pushes two sentinel
    /// edges (indices 0 and 1) that bound the AET.
    pub fn reset(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_reset");
        self.edges.clear();
        self.surfaces.clear();
        self.spans.clear();
        self.polygon_data.clear();
        self.new_edges.fill(EDGE_NONE);
        self.remove_edges.fill(EDGE_NONE);
        self.surface_stack.clear();
        self.next_key = 0;
        self.active_count = 0;
        self.stats.reset();

        // Sentinel 0: left guard (x = -1)
        self.edges.push(Edge {
            x: -1.0,
            x_step: 0.0,
            surfs: [SURF_NONE, SURF_NONE],
            inv_w: 0.0,
            inv_w_step: 0.0,
            prev: EDGE_NONE,
            next: AET_TAIL,
            next_new: EDGE_NONE,
            next_remove: EDGE_NONE,
        });
        // Sentinel 1: right guard (x = infinity, catches all edges)
        self.edges.push(Edge {
            x: f32::MAX,
            x_step: 0.0,
            surfs: [SURF_NONE, SURF_NONE],
            inv_w: 0.0,
            inv_w_step: 0.0,
            prev: AET_HEAD,
            next: EDGE_NONE,
            next_new: EDGE_NONE,
            next_remove: EDGE_NONE,
        });
    }

    /// Emit edges for a single visible polygon. Called during BSP traversal
    /// after frustum clipping and screen projection.
    pub fn emit_polygon(
        &mut self,
        screen_verts: &[Vec2],
        inv_w: &[f32],
        polygon: *const SurfacePolygon,
        interpolator: TriangleInterpolator,
        brightness: usize,
        is_sky: bool,
    ) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_emit");
        let vert_count = screen_verts.len();
        if vert_count < 3 {
            return;
        }

        let surface_idx = self.surfaces.len();
        let polygon_idx = self.polygon_data.len();

        self.polygon_data.push(PolygonRenderData {
            polygon,
            interpolator,
            brightness,
            is_sky,
        });

        self.surfaces.push(SpanSurface {
            key: self.next_key,
            polygon_idx,
            last_u: 0,
            span_head: SPAN_END,
            spanstate: 0,
            last_inv_w: 0.0,
        });
        self.next_key += 1;
        self.stats.surfaces_emitted += 1;

        let h = self.height as f32;
        for i in 0..vert_count {
            let ni = (i + 1) % vert_count;
            let v0 = screen_verts[i];
            let v1 = screen_verts[ni];
            let iw0 = inv_w[i];
            let iw1 = inv_w[ni];

            let dy = v1.y - v0.y;
            if dy.abs() < 0.001 {
                continue;
            }

            let (top, bot, iw_top, iw_bot, is_leading) = if dy > 0.0 {
                (v0, v1, iw0, iw1, true)
            } else {
                (v1, v0, iw1, iw0, false)
            };

            let y_start = top.y.ceil().max(0.0) as usize;
            let y_end = bot.y.ceil().min(h) as usize;
            if y_start >= y_end {
                continue;
            }

            let edge_dy = bot.y - top.y;
            let x_step = (bot.x - top.x) / edge_dy;
            let inv_w_step = (iw_bot - iw_top) / edge_dy;
            let prestep = y_start as f32 - top.y;
            let x = top.x + x_step * prestep;
            let edge_inv_w = iw_top + inv_w_step * prestep;

            let surfs = if is_leading {
                [SURF_NONE, surface_idx]
            } else {
                [surface_idx, SURF_NONE]
            };

            let edge_idx = self.edges.len();
            // Link into per-scanline new-edge chain
            let old_new_head = self.new_edges[y_start];
            self.edges.push(Edge {
                x,
                x_step,
                surfs,
                inv_w: edge_inv_w,
                inv_w_step,
                prev: EDGE_NONE,
                next: EDGE_NONE,
                next_new: old_new_head,
                next_remove: EDGE_NONE,
            });
            self.new_edges[y_start] = edge_idx;

            // Link into per-scanline removal chain
            let remove_y = y_end - 1;
            if remove_y < self.height {
                self.edges[edge_idx].next_remove = self.remove_edges[remove_y];
                self.remove_edges[remove_y] = edge_idx;
            }

            self.stats.edges_emitted += 1;
        }
    }

    /// Insert an edge into the AET at the correct x-sorted position.
    /// Walks from the left sentinel rightward; right sentinel (f32::MAX)
    /// guarantees termination.
    #[inline]
    fn aet_insert(&mut self, edge_idx: usize) {
        let edge_x = self.edges[edge_idx].x;
        let mut cursor = self.edges[AET_HEAD].next;
        while self.edges[cursor].x <= edge_x {
            cursor = self.edges[cursor].next;
        }
        let prev = self.edges[cursor].prev;
        self.edges[edge_idx].prev = prev;
        self.edges[edge_idx].next = cursor;
        self.edges[prev].next = edge_idx;
        self.edges[cursor].prev = edge_idx;
        self.active_count += 1;
    }

    /// Remove an edge from the AET. O(1) unlink via prev/next.
    #[inline]
    fn aet_remove(&mut self, edge_idx: usize) {
        let prev = self.edges[edge_idx].prev;
        let next = self.edges[edge_idx].next;
        self.edges[prev].next = next;
        self.edges[next].prev = prev;
        self.active_count -= 1;
    }

    /// Re-sort the AET after stepping edges. Walks the list and bubbles any
    /// out-of-order edge leftward to its correct position. Left sentinel
    /// (x=-1) acts as a natural stop for the leftward walk.
    fn aet_resort(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_resort");
        let mut idx = self.edges[AET_HEAD].next;
        while idx != AET_TAIL {
            let next = self.edges[idx].next;
            let prev = self.edges[idx].prev;
            if self.edges[prev].x > self.edges[idx].x {
                // Unlink from current position
                self.edges[prev].next = next;
                self.edges[next].prev = prev;
                // Walk left to find correct position; left sentinel (x=-1)
                // guarantees termination.
                let mut insert_after = self.edges[prev].prev;
                while self.edges[insert_after].x > self.edges[idx].x {
                    insert_after = self.edges[insert_after].prev;
                }
                let insert_before = self.edges[insert_after].next;
                self.edges[idx].prev = insert_after;
                self.edges[idx].next = insert_before;
                self.edges[insert_after].next = idx;
                self.edges[insert_before].prev = idx;
            }
            idx = next;
        }
    }

    /// Process all scanlines: insert new edges, generate spans, remove
    /// expired edges, step and re-sort.
    pub fn process_scanlines(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_process");
        for y in 0..self.height {
            // Walk per-scanline new-edge chain, insert each into AET
            let mut new_idx = self.new_edges[y];
            while new_idx != EDGE_NONE {
                let next_new = self.edges[new_idx].next_new;
                self.aet_insert(new_idx);
                new_idx = next_new;
            }

            if self.active_count > self.stats.max_active_edges {
                self.stats.max_active_edges = self.active_count;
            }

            self.generate_spans(y);

            // Remove edges expiring at this scanline — O(1) unlink each
            let mut remove_idx = self.remove_edges[y];
            while remove_idx != EDGE_NONE {
                let next_remove = self.edges[remove_idx].next_remove;
                self.aet_remove(remove_idx);
                remove_idx = next_remove;
            }

            // Step active edges to next scanline
            let mut idx = self.edges[AET_HEAD].next;
            while idx != AET_TAIL {
                let edge = &mut self.edges[idx];
                let next = edge.next;
                edge.x += edge.x_step;
                edge.inv_w += edge.inv_w_step;
                idx = next;
            }

            // Re-sort after stepping (edges may have crossed)
            self.aet_resort();
        }
    }

    /// Walk the AET left-to-right via the intrusive linked list, using the
    /// surface stack to determine visibility and emit spans.
    fn generate_spans(&mut self, y: usize) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_gen");
        let mut idx = self.edges[AET_HEAD].next;
        while idx != AET_TAIL {
            let edge = &self.edges[idx];
            let edge_x = edge.x;
            let edge_inv_w = edge.inv_w;
            let surfs = edge.surfs;
            let next = edge.next;

            if surfs[0] != SURF_NONE {
                self.trailing_edge(surfs[0], edge_x, edge_inv_w, y);
            }

            if surfs[1] != SURF_NONE {
                self.leading_edge(surfs[1], edge_x, edge_inv_w, y);
            }

            idx = next;
        }
    }

    /// Handle a leading edge: surface is entering the active region.
    fn leading_edge(&mut self, surf_idx: usize, edge_x: f32, edge_inv_w: f32, y: usize) {
        let surf = &mut self.surfaces[surf_idx];
        surf.spanstate += 1;
        if surf.spanstate != 1 {
            return;
        }

        let surf_key = surf.key;
        let ix = edge_x.max(0.0).min(self.w_max) as i32;

        let stack_pos = self
            .surface_stack
            .iter()
            .position(|&idx| self.surfaces[idx].key > surf_key)
            .unwrap_or(self.surface_stack.len());

        if stack_pos == 0 {
            if let Some(&old_front_idx) = self.surface_stack.first() {
                let old_front = &self.surfaces[old_front_idx];
                let old_last_u = old_front.last_u;
                let old_inv_w = old_front.last_inv_w;
                if ix > old_last_u {
                    self.emit_span(old_front_idx, old_last_u, ix, old_inv_w, edge_inv_w, y);
                }
            }
        }

        self.surface_stack.insert(stack_pos, surf_idx);
        let sd = self.surface_stack.len();
        if sd > self.stats.max_stack_depth {
            self.stats.max_stack_depth = sd;
        }

        let surf = &mut self.surfaces[surf_idx];
        surf.last_u = ix;
        surf.last_inv_w = edge_inv_w;
    }

    /// Handle a trailing edge: surface is leaving the active region.
    fn trailing_edge(&mut self, surf_idx: usize, edge_x: f32, edge_inv_w: f32, y: usize) {
        let surf = &mut self.surfaces[surf_idx];
        surf.spanstate -= 1;
        if surf.spanstate != 0 {
            return;
        }

        let ix = edge_x.max(0.0).min(self.w_max) as i32;

        let stack_pos = self.surface_stack.iter().position(|&idx| idx == surf_idx);

        if let Some(pos) = stack_pos {
            if pos == 0 {
                let surf = &self.surfaces[surf_idx];
                let last_u = surf.last_u;
                let last_inv_w = surf.last_inv_w;
                if ix > last_u {
                    self.emit_span(surf_idx, last_u, ix, last_inv_w, edge_inv_w, y);
                }

                if self.surface_stack.len() > 1 {
                    let new_front_idx = self.surface_stack[1];
                    let new_front = &mut self.surfaces[new_front_idx];
                    new_front.last_u = ix;
                    new_front.last_inv_w = edge_inv_w;
                }
            }

            self.surface_stack.remove(pos);
        }
    }

    /// Emit a span for a surface.
    fn emit_span(
        &mut self,
        surf_idx: usize,
        x_start: i32,
        x_end: i32,
        inv_w_start: f32,
        inv_w_end: f32,
        y: usize,
    ) {
        if x_start >= x_end {
            return;
        }

        let xs = x_start.max(0) as usize;
        let xe = (x_end as usize).min(self.width);
        if xs >= xe {
            return;
        }

        let span_idx = self.spans.len();
        let surf = &mut self.surfaces[surf_idx];
        let old_head = surf.span_head;

        self.spans.push(Span {
            x_start: xs,
            x_end: xe,
            y,
            inv_w_start,
            inv_w_end,
            next: old_head,
        });

        self.surfaces[surf_idx].span_head = span_idx;
        self.stats.spans_generated += 1;
    }

    /// Draw all accumulated spans. For each surface, walks its span list and
    /// paints pixels using the polygon's interpolation data.
    pub fn draw_spans(
        &self,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
        depth_buffer: &mut DepthBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_draw");
        let sky_pic = pic_data.sky_pic();
        let sky_num = pic_data.sky_num();

        for surf in &self.surfaces {
            if surf.span_head == SPAN_END {
                continue;
            }

            let poly_data = &self.polygon_data[surf.polygon_idx];
            let polygon = unsafe { &*poly_data.polygon };
            let texture_sampler =
                TextureSampler::new(&polygon.surface_kind, pic_data, sky_pic, sky_num);
            let interpolator = &poly_data.interpolator;
            let brightness = poly_data.brightness;
            let is_sky = poly_data.is_sky;

            let mut span_idx = surf.span_head;
            while span_idx != SPAN_END {
                let span = &self.spans[span_idx];
                let y = span.y;
                let x_start = span.x_start;
                let x_end = span.x_end;
                let span_width = x_end - x_start;

                if span_width == 0 {
                    span_idx = span.next;
                    continue;
                }

                if is_sky {
                    for x in x_start..x_end {
                        depth_buffer.set_sky_depth_unchecked(x, y);
                    }
                    span_idx = span.next;
                    continue;
                }

                let inv_w_dx = if span_width > 1 {
                    (span.inv_w_end - span.inv_w_start) / span_width as f32
                } else {
                    0.0
                };

                let mut interp_state = interpolator.init_scanline(x_start as f32, y as f32);
                let mut edge_inv_w = span.inv_w_start;

                for x in x_start..x_end {
                    let depth = edge_inv_w.max(MIN_GEOMETRY_DEPTH);
                    depth_buffer.set_depth_unchecked(x, y, depth);

                    let (u, v) = interp_state.get_current_uv();
                    let colourmap = pic_data.base_colourmap(brightness, edge_inv_w * LIGHT_SCALE);
                    let color = texture_sampler.sample(u, v, colourmap, pic_data);

                    write_pixel(buffer, x, y, color, None);

                    interp_state.step_x();
                    edge_inv_w += inv_w_dx;
                }

                span_idx = span.next;
            }
        }
    }
}
