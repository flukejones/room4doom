#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::{
    BSP3D, PicData, PvsData, Sector, SurfaceKind, SurfacePolygon, WallType, is_subsector,
    subsector_index,
};
use glam::{Vec2, Vec3, Vec4};
use render_trait::{DrawBuffer, SOFT_PIXEL_CHANNELS};

use std::alloc::{self, Layout};
use std::ptr;

use crate::render::{TextureSampler, TriangleInterpolator, sample_sky_pixel};
use crate::{MAX_CLIPPED_VERTICES, SkyRend, Software3D};

const TILE_SIZE: usize = 8;
const LIGHT_MIN_Z: f32 = 0.001;
const LIGHT_MAX_Z: f32 = 0.055;
const LIGHT_RANGE: f32 = 1.0 / (LIGHT_MAX_Z - LIGHT_MIN_Z);
const LIGHT_SCALE: f32 = LIGHT_RANGE * 8.0 * 16.0;
/// Default span pool capacity. After a pre-scanline flush, a single
/// scanline's spans (up to max_active_edges) must fit in the remaining space.
const SPAN_POOL_CAPACITY: usize = 8192;
/// Flush before a scanline if fewer than this many slots remain.
const SPAN_FLUSH_MARGIN: usize = 4096;

/// Fixed-capacity bump allocator for frame-scoped objects. Single contiguous
/// allocation, no per-push capacity checks, no reallocation. Reset each frame
/// by setting `len = 0`.
struct BumpPool<T> {
    ptr: *mut T,
    len: usize,
    capacity: usize,
}

impl<T> BumpPool<T> {
    /// Allocate a pool with the given capacity. Panics if allocation fails.
    fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        let layout = Layout::array::<T>(capacity).expect("layout overflow");
        let ptr = unsafe { alloc::alloc(layout) as *mut T };
        assert!(!ptr.is_null(), "allocation failed");
        Self {
            ptr,
            len: 0,
            capacity,
        }
    }

    /// Reset the pool without dropping elements (they're trivially copyable
    /// structs with no Drop impl).
    #[inline(always)]
    fn clear(&mut self) {
        self.len = 0;
    }

    /// Number of live elements.
    #[inline(always)]
    fn len(&self) -> usize {
        self.len
    }

    /// Push an element, returning a raw pointer to it. No capacity check —
    /// caller must ensure the pool was sized correctly.
    #[inline(always)]
    unsafe fn push_unchecked(&mut self, val: T) -> *mut T {
        let idx = self.len;
        debug_assert!(idx < self.capacity);
        let p = unsafe { self.ptr.add(idx) };
        unsafe { p.write(val) };
        self.len = idx + 1;
        p
    }

    /// Iterate over live elements.
    fn iter(&self) -> std::slice::Iter<'_, T> {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len).iter() }
    }
}

impl<T> Drop for BumpPool<T> {
    fn drop(&mut self) {
        if self.capacity > 0 {
            let layout = Layout::array::<T>(self.capacity).unwrap();
            unsafe { alloc::dealloc(self.ptr as *mut u8, layout) };
        }
    }
}

/// An edge in screen space, tracking one side of a polygon across scanlines.
/// Doubly-linked into the active edge table (AET) via `prev`/`next`.
struct Edge {
    /// Current screen-space x position (sub-pixel precision).
    x: f32,
    /// X increment per scanline.
    x_step: f32,
    /// Surface pointers: [0] = trailing (surface leaving), [1] = leading
    /// (surface entering). Null means no surface on that side.
    surfs: [*mut SpanSurface; 2],
    /// Previous edge in AET doubly-linked list (null = not in AET).
    prev: *mut Edge,
    /// Next edge in AET doubly-linked list (null = not in AET).
    next: *mut Edge,
    /// Next edge in the per-scanline new-edge chain (null = end).
    next_new: *mut Edge,
    /// Next edge in the per-scanline removal chain (null = end).
    next_remove: *mut Edge,
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
    /// Head of the span linked list.
    span_head: *mut Span,
    /// Edge pair counter: 0 = not on stack, 1 = active.
    spanstate: i32,
    /// Screen-space 1/w plane equation: inv_w = origin + x * step_x + y *
    /// step_y. Computed once per polygon from 3 screen-space vertices.
    inv_w_origin: f32,
    /// Per-pixel x step for 1/w.
    inv_w_step_x: f32,
    /// Per-scanline y step for 1/w.
    inv_w_step_y: f32,
    /// Previous surface in the surface stack (null = head/not linked).
    prev: *mut SpanSurface,
    /// Next surface in the surface stack (null = tail/not linked).
    next: *mut SpanSurface,
}

/// A horizontal span of pixels belonging to one surface on one scanline.
pub struct Span {
    /// Start x pixel (inclusive).
    x_start: usize,
    /// End x pixel (exclusive).
    x_end: usize,
    /// Scanline y.
    y: usize,
    /// Next span for the same surface (linked list).
    next: *mut Span,
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
    pub span_flushes: usize,
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
            "edges: {} | surfs: {} | spans: {} | max_active: {} | max_stack: {} | flushes: {}",
            self.edges_emitted,
            self.surfaces_emitted,
            self.spans_generated,
            self.max_active_edges,
            self.max_stack_depth,
            self.span_flushes,
        )
    }
}

/// Edge-based span generation state. Manages all per-frame edge, surface, and
/// span data for Quake-style rasterisation.
///
/// Storage: 3 object pools (`edges`, `surfaces`, `spans`) + 1 polygon data
/// pool + 2 per-scanline chain head arrays (`new_edges`, `remove_edges`).
/// Active edges form an intrusive doubly-linked list through `Edge::prev`/
/// `next`, bounded by sentinels pointed to by `aet_head`/`aet_tail`.
pub struct EdgeSpanState {
    /// Edge pool. First two pushes are sentinels; real edges follow.
    edges: BumpPool<Edge>,
    /// Surface pool.
    surfaces: BumpPool<SpanSurface>,
    /// Span pool. Flushed mid-frame when nearing capacity.
    spans: BumpPool<Span>,
    /// Span count at which a mid-frame flush is triggered.
    span_flush_threshold: usize,
    /// Per-polygon render data pool.
    pub polygon_data: Vec<PolygonRenderData>,
    /// Per-scanline new-edge chain heads. Each entry is the head of a
    /// singly-linked list via `Edge::next_new`.
    new_edges: Vec<*mut Edge>,
    /// Per-scanline removal chain heads. Each entry is the head of a
    /// singly-linked list via `Edge::next_remove`.
    remove_edges: Vec<*mut Edge>,
    /// Left sentinel edge pointer.
    aet_head: *mut Edge,
    /// Right sentinel edge pointer.
    aet_tail: *mut Edge,
    /// Current number of real (non-sentinel) edges in the AET.
    active_count: usize,
    /// Head of the intrusive surface stack (lowest key = closest).
    /// Null when empty.
    surf_stack_head: *mut SpanSurface,
    /// Current depth of the surface stack.
    stack_depth: usize,
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
            edges: BumpPool::new(16384),
            surfaces: BumpPool::new(8192),
            spans: BumpPool::new(SPAN_POOL_CAPACITY),
            span_flush_threshold: SPAN_POOL_CAPACITY - SPAN_FLUSH_MARGIN,
            polygon_data: Vec::with_capacity(512),
            new_edges: vec![ptr::null_mut(); h],
            remove_edges: vec![ptr::null_mut(); h],
            aet_head: ptr::null_mut(),
            aet_tail: ptr::null_mut(),
            active_count: 0,
            surf_stack_head: ptr::null_mut(),
            stack_depth: 0,
            next_key: 0,
            width: w,
            height: h,
            w_max: w as f32,
            stats: EdgeSpanStats::default(),
        }
    }

    /// Reset all per-frame state. Retains allocated memory. Pushes two sentinel
    /// edges that bound the AET.
    pub fn reset(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_reset");
        self.edges.clear();
        self.surfaces.clear();
        self.spans.clear();
        self.polygon_data.clear();
        self.new_edges.fill(ptr::null_mut());
        self.remove_edges.fill(ptr::null_mut());
        self.surf_stack_head = ptr::null_mut();
        self.stack_depth = 0;
        self.next_key = 0;
        self.active_count = 0;
        self.stats.reset();

        // Sentinel 0: left guard (x = -1)
        let head = unsafe {
            self.edges.push_unchecked(Edge {
                x: -1.0,
                x_step: 0.0,
                surfs: [ptr::null_mut(), ptr::null_mut()],
                prev: ptr::null_mut(),
                next: ptr::null_mut(), // patched below
                next_new: ptr::null_mut(),
                next_remove: ptr::null_mut(),
            })
        };
        // Sentinel 1: right guard (x = infinity)
        let tail = unsafe {
            self.edges.push_unchecked(Edge {
                x: f32::MAX,
                x_step: 0.0,
                surfs: [ptr::null_mut(), ptr::null_mut()],
                prev: head,
                next: ptr::null_mut(),
                next_new: ptr::null_mut(),
                next_remove: ptr::null_mut(),
            })
        };
        unsafe { (*head).next = tail };
        self.aet_head = head;
        self.aet_tail = tail;
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

        // Skip polygon if pools lack capacity (massive maps).
        if self.surfaces.len >= self.surfaces.capacity
            || self.edges.len + vert_count >= self.edges.capacity
        {
            return;
        }

        let polygon_idx = self.polygon_data.len();

        self.polygon_data.push(PolygonRenderData {
            polygon,
            interpolator,
            brightness,
            is_sky,
        });

        // Compute screen-space 1/w plane equation. Pick the vertex triple
        // with the largest screen-space cross product for numerical stability
        // (avoids near-collinear first-3-vertex degeneracy in N-gons).
        let (inv_w_origin, inv_w_step_x, inv_w_step_y) = {
            let mut best_denom = 0.0f32;
            let mut best_i = (0, 1, 2);
            for i in 0..vert_count {
                for j in (i + 1)..vert_count {
                    for k in (j + 1)..vert_count {
                        let d1x = screen_verts[j].x - screen_verts[i].x;
                        let d1y = screen_verts[j].y - screen_verts[i].y;
                        let d2x = screen_verts[k].x - screen_verts[i].x;
                        let d2y = screen_verts[k].y - screen_verts[i].y;
                        let d = (d1x * d2y - d2x * d1y).abs();
                        if d > best_denom {
                            best_denom = d;
                            best_i = (i, j, k);
                        }
                    }
                }
            }
            if best_denom < 1e-6 {
                (inv_w[0], 0.0, 0.0)
            } else {
                let (i0, i1, i2) = best_i;
                let p0 = screen_verts[i0];
                let p1 = screen_verts[i1];
                let p2 = screen_verts[i2];
                let d1x = p1.x - p0.x;
                let d1y = p1.y - p0.y;
                let d1w = inv_w[i1] - inv_w[i0];
                let d2x = p2.x - p0.x;
                let d2y = p2.y - p0.y;
                let d2w = inv_w[i2] - inv_w[i0];
                let denom = d1x * d2y - d2x * d1y;
                let inv_denom = 1.0 / denom;
                let sx = (d1w * d2y - d2w * d1y) * inv_denom;
                let sy = (d2w * d1x - d1w * d2x) * inv_denom;
                let o = inv_w[i0] - sx * p0.x - sy * p0.y;
                (o, sx, sy)
            }
        };

        let surf_ptr = unsafe {
            self.surfaces.push_unchecked(SpanSurface {
                key: self.next_key,
                polygon_idx,
                last_u: 0,
                span_head: ptr::null_mut(),
                spanstate: 0,
                inv_w_origin,
                inv_w_step_x,
                inv_w_step_y,
                prev: ptr::null_mut(),
                next: ptr::null_mut(),
            })
        };
        self.next_key += 1;
        self.stats.surfaces_emitted += 1;

        let h = self.height as f32;
        for i in 0..vert_count {
            let ni = (i + 1) % vert_count;
            let v0 = screen_verts[i];
            let v1 = screen_verts[ni];

            let dy = v1.y - v0.y;
            if dy.abs() < 0.001 {
                continue;
            }

            let (top, bot, is_leading) = if dy > 0.0 {
                (v0, v1, true)
            } else {
                (v1, v0, false)
            };

            let y_start = top.y.ceil().max(0.0) as usize;
            let y_end = bot.y.ceil().min(h) as usize;
            if y_start >= y_end {
                continue;
            }

            let edge_dy = bot.y - top.y;
            let x_step = (bot.x - top.x) / edge_dy;
            let prestep = y_start as f32 - top.y;
            let x = top.x + x_step * prestep;

            let surfs = if is_leading {
                [ptr::null_mut(), surf_ptr]
            } else {
                [surf_ptr, ptr::null_mut()]
            };

            let edge_ptr = unsafe {
                self.edges.push_unchecked(Edge {
                    x,
                    x_step,
                    surfs,
                    prev: ptr::null_mut(),
                    next: ptr::null_mut(),
                    next_new: ptr::null_mut(),
                    next_remove: ptr::null_mut(),
                })
            };

            // Insertion-sort into per-scanline new-edge chain by x.
            let mut cursor = self.new_edges[y_start];
            let mut prev_cursor: *mut Edge = ptr::null_mut();
            while !cursor.is_null() {
                let cx = unsafe { (*cursor).x };
                if cx > x {
                    break;
                }
                prev_cursor = cursor;
                cursor = unsafe { (*cursor).next_new };
            }
            unsafe { (*edge_ptr).next_new = cursor };
            if !prev_cursor.is_null() {
                unsafe { (*prev_cursor).next_new = edge_ptr };
            } else {
                self.new_edges[y_start] = edge_ptr;
            }

            // Link into per-scanline removal chain.
            let remove_y = y_end - 1;
            if remove_y < self.height {
                unsafe {
                    (*edge_ptr).next_remove = self.remove_edges[remove_y];
                }
                self.remove_edges[remove_y] = edge_ptr;
            }

            self.stats.edges_emitted += 1;
        }
    }

    /// Sorted merge: splice a pre-sorted new-edge chain into the AET.
    /// Both lists are sorted by x, so this is a single left-to-right pass.
    ///
    /// SAFETY: all edge pointers in the new-edge chain and the AET must be
    /// valid.
    fn aet_merge_new_edges(&mut self, mut new_ptr: *mut Edge) {
        let tail = self.aet_tail;
        let mut cursor = unsafe { (*self.aet_head).next };
        while !new_ptr.is_null() {
            let next_new = unsafe { (*new_ptr).next_new };
            let new_x = unsafe { (*new_ptr).x };
            // Advance AET cursor past edges with x <= new_x
            while cursor != tail && unsafe { (*cursor).x } <= new_x {
                cursor = unsafe { (*cursor).next };
            }
            let prev = unsafe { (*cursor).prev };
            unsafe {
                (*new_ptr).prev = prev;
                (*new_ptr).next = cursor;
                (*prev).next = new_ptr;
                (*cursor).prev = new_ptr;
            }
            self.active_count += 1;
            new_ptr = next_new;
        }
    }

    /// Remove an edge from the AET. O(1) unlink via prev/next.
    ///
    /// SAFETY: `edge` must be a valid pointer to an edge in the AET.
    #[inline(always)]
    unsafe fn aet_remove(&mut self, edge: *mut Edge) {
        unsafe {
            let prev = (*edge).prev;
            let next = (*edge).next;
            (*prev).next = next;
            (*next).prev = prev;
        }
        self.active_count -= 1;
    }

    /// Step all active edges to the next scanline and fix sort order.
    /// Combined step + resort in one AET pass. Edges rarely cross between
    /// scanlines, so the backward scan is almost always 0–1 hops.
    ///
    /// SAFETY: AET linked list must be well-formed with valid sentinels.
    fn aet_step_and_resort(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_resort");
        let tail = self.aet_tail;
        let mut edge = unsafe { (*self.aet_head).next };
        while edge != tail {
            let e = unsafe { &mut *edge };
            e.x += e.x_step;
            let next = e.next;
            let prev = e.prev;
            let new_x = e.x;
            if unsafe { (*prev).x } > new_x {
                // Unlink
                unsafe {
                    (*prev).next = next;
                    (*next).prev = prev;
                }
                // Walk left; left sentinel (x=-1) guarantees termination
                let mut ia = unsafe { (*prev).prev };
                while unsafe { (*ia).x } > new_x {
                    ia = unsafe { (*ia).prev };
                }
                let ib = unsafe { (*ia).next };
                unsafe {
                    (*edge).prev = ia;
                    (*edge).next = ib;
                    (*ia).next = edge;
                    (*ib).prev = edge;
                }
            }
            edge = next;
        }
    }

    /// Process all scanlines and draw spans. Flushes the span pool mid-frame
    /// when it approaches capacity, drawing accumulated spans before clearing.
    ///
    /// `depth_ptr` and `depth_stride`: raw pointer to depth buffer and row
    /// stride. Depth is written for every non-sky pixel so masked walls can
    /// depth-test against span-rendered geometry.
    ///
    /// SAFETY: `depth_ptr` must be valid for `height * depth_stride` f32
    /// writes.
    pub fn process_and_draw_spans(
        &mut self,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
        depth_ptr: *mut f32,
        depth_stride: usize,
        tile_min_ptr: *mut f32,
        tile_covered_ptr: *mut u16,
        tiles_x: usize,
        sky: &SkyRend,
    ) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_process");
        let flush_threshold = self.span_flush_threshold;
        for y in 0..self.height {
            let new_head = unsafe { *self.new_edges.get_unchecked(y) };
            if !new_head.is_null() {
                self.aet_merge_new_edges(new_head);
            }

            if self.active_count > self.stats.max_active_edges {
                self.stats.max_active_edges = self.active_count;
            }

            // Flush spans before generating new ones if pool is nearing capacity
            if self.spans.len() >= flush_threshold {
                self.draw_spans(
                    pic_data,
                    buffer,
                    depth_ptr,
                    depth_stride,
                    tile_min_ptr,
                    tile_covered_ptr,
                    tiles_x,
                    sky,
                );
                self.flush_spans();
            }

            self.generate_spans(y);
            self.cleanup_scanline(y);

            let mut remove_ptr = unsafe { *self.remove_edges.get_unchecked(y) };
            while !remove_ptr.is_null() {
                let next_remove = unsafe { (*remove_ptr).next_remove };
                unsafe { self.aet_remove(remove_ptr) };
                remove_ptr = next_remove;
            }

            self.aet_step_and_resort();
        }

        // Draw remaining spans
        self.draw_spans(
            pic_data,
            buffer,
            depth_ptr,
            depth_stride,
            tile_min_ptr,
            tile_covered_ptr,
            tiles_x,
            sky,
        );
    }

    /// Clear span pool and reset all surface span_head pointers.
    fn flush_spans(&mut self) {
        self.spans.clear();
        // Reset span_head on all live surfaces
        let surfs_ptr = self.surfaces.ptr;
        let surfs_len = self.surfaces.len;
        for i in 0..surfs_len {
            unsafe { (*surfs_ptr.add(i)).span_head = ptr::null_mut() };
        }
        self.stats.span_flushes += 1;
    }

    /// Walk the AET left-to-right via the intrusive linked list, using the
    /// surface stack to determine visibility and emit spans.
    ///
    /// SAFETY: AET linked list and surface stack must be well-formed.
    fn generate_spans(&mut self, y: usize) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_gen");
        let tail = self.aet_tail;
        let w_max = self.w_max;
        let mut edge = unsafe { (*self.aet_head).next };
        while edge != tail {
            let e = unsafe { &*edge };
            let surfs = e.surfs;
            let next = e.next;
            // Clamp once, reuse for both leading and trailing
            let ix = e.x.max(0.0).min(w_max) as i32;

            if !surfs[0].is_null() {
                self.trailing_edge(surfs[0], ix, y);
            }

            if !surfs[1].is_null() {
                self.leading_edge(surfs[1], ix, y);
            }

            edge = next;
        }
    }

    /// Emit a final span for whatever surface is on top of the stack at the
    /// right screen edge. Equivalent to Quake's R_CleanupSpan.
    #[inline(always)]
    fn cleanup_scanline(&mut self, y: usize) {
        let head = self.surf_stack_head;
        if !head.is_null() {
            let last_u = unsafe { (*head).last_u };
            let right = self.width as i32;
            if right > last_u {
                self.emit_span(head, last_u, right, y);
            }
        }
    }

    /// Handle a leading edge: surface is entering the active region.
    /// Inserts into intrusive surface stack sorted by key (ascending).
    ///
    /// SAFETY: `surf` must be a valid pointer into `self.surfaces`.
    #[inline(always)]
    fn leading_edge(&mut self, surf: *mut SpanSurface, ix: i32, y: usize) {
        let s = unsafe { &mut *surf };
        s.spanstate += 1;
        if s.spanstate != 1 {
            return;
        }

        let surf_key = s.key;
        let head = self.surf_stack_head;

        if head.is_null() {
            // Empty stack — just insert
            self.surf_stack_head = surf;
            unsafe {
                (*surf).prev = ptr::null_mut();
                (*surf).next = ptr::null_mut();
                (*surf).last_u = ix;
            }
            self.stack_depth = 1;
            if self.stack_depth > self.stats.max_stack_depth {
                self.stats.max_stack_depth = self.stack_depth;
            }
            return;
        }

        let head_key = unsafe { (*head).key };

        if surf_key < head_key {
            // New surface goes in front — emit span for old head
            let old_last_u = unsafe { (*head).last_u };
            if ix > old_last_u {
                self.emit_span(head, old_last_u, ix, y);
            }
            // Insert at head
            unsafe {
                (*surf).prev = ptr::null_mut();
                (*surf).next = head;
                (*head).prev = surf;
                (*surf).last_u = ix;
            }
            self.surf_stack_head = surf;
        } else {
            // Walk to find insertion point (common case: goes behind head)
            let mut prev_cursor = head;
            let mut cursor = unsafe { (*head).next };
            while !cursor.is_null() {
                let ck = unsafe { (*cursor).key };
                if ck > surf_key {
                    break;
                }
                prev_cursor = cursor;
                cursor = unsafe { (*cursor).next };
            }
            // Insert after prev_cursor
            unsafe {
                (*surf).prev = prev_cursor;
                (*surf).next = cursor;
                (*prev_cursor).next = surf;
                (*surf).last_u = ix;
            }
            if !cursor.is_null() {
                unsafe { (*cursor).prev = surf };
            }
        }

        self.stack_depth += 1;
        if self.stack_depth > self.stats.max_stack_depth {
            self.stats.max_stack_depth = self.stack_depth;
        }
    }

    /// Handle a trailing edge: surface is leaving the active region.
    /// Unlinks from intrusive surface stack via prev/next.
    ///
    /// SAFETY: `surf` must be a valid pointer into `self.surfaces`.
    #[inline(always)]
    fn trailing_edge(&mut self, surf: *mut SpanSurface, ix: i32, y: usize) {
        let s = unsafe { &mut *surf };
        s.spanstate -= 1;
        if s.spanstate != 0 {
            return;
        }

        if self.surf_stack_head == surf {
            let last_u = s.last_u;
            if ix > last_u {
                self.emit_span(surf, last_u, ix, y);
            }
            let new_head = s.next;
            if !new_head.is_null() {
                unsafe {
                    (*new_head).last_u = ix;
                }
            }
        }

        // Unlink
        let prev = unsafe { (*surf).prev };
        let next = unsafe { (*surf).next };
        if !prev.is_null() {
            unsafe { (*prev).next = next };
        } else {
            self.surf_stack_head = next;
        }
        if !next.is_null() {
            unsafe { (*next).prev = prev };
        }
        unsafe {
            (*surf).prev = ptr::null_mut();
            (*surf).next = ptr::null_mut();
        }
        self.stack_depth -= 1;
    }

    /// Emit a span for a surface. Uses pre-reserved capacity to avoid
    /// per-push bounds checks.
    ///
    /// SAFETY: `surf` must be a valid pointer into `self.surfaces`.
    /// Caller must ensure `self.spans` has spare capacity.
    #[inline(always)]
    fn emit_span(&mut self, surf: *mut SpanSurface, x_start: i32, x_end: i32, y: usize) {
        if x_start >= x_end {
            return;
        }

        let xs = x_start.max(0) as usize;
        let xe = (x_end as usize).min(self.width);
        if xs >= xe {
            return;
        }

        let old_head = unsafe { (*surf).span_head };

        let span_ptr = unsafe {
            self.spans.push_unchecked(Span {
                x_start: xs,
                x_end: xe,
                y,
                next: old_head,
            })
        };

        unsafe { (*surf).span_head = span_ptr };
        self.stats.spans_generated += 1;
    }

    /// Draw all accumulated spans. For each surface, walks its span list and
    /// paints pixels using the polygon's interpolation data. Depth (1/w) is
    /// evaluated from the per-surface screen-space plane equation and written
    /// to the depth buffer for subsequent masked wall depth testing.
    ///
    /// SAFETY: `depth_ptr` must be valid for writes at offsets up to
    /// `(height-1) * depth_stride + (width-1)`.
    fn draw_spans(
        &self,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
        depth_ptr: *mut f32,
        depth_stride: usize,
        tile_min_ptr: *mut f32,
        tile_covered_ptr: *mut u16,
        tiles_x: usize,
        sky: &SkyRend,
    ) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_draw");
        let sky_pic = pic_data.sky_pic();
        let sky_num = pic_data.sky_num();
        let pitch = buffer.pitch();
        let buf = buffer.buf_mut();

        for surf in self.surfaces.iter() {
            if surf.span_head.is_null() {
                continue;
            }

            let poly_data = &self.polygon_data[surf.polygon_idx];
            let polygon = unsafe { &*poly_data.polygon };
            let texture_sampler =
                TextureSampler::new(&polygon.surface_kind, pic_data, sky_pic, sky_num);
            let interpolator = &poly_data.interpolator;
            let brightness = poly_data.brightness;
            let is_sky = poly_data.is_sky;

            let inv_w_origin = surf.inv_w_origin;
            let inv_w_step_x = surf.inv_w_step_x;
            let inv_w_step_y = surf.inv_w_step_y;

            let mut span_ptr = surf.span_head;
            while !span_ptr.is_null() {
                let span = unsafe { &*span_ptr };
                let y = span.y;
                let x_start = span.x_start;
                let x_end = span.x_end;
                let span_width = x_end - x_start;

                if span_width == 0 {
                    span_ptr = span.next;
                    continue;
                }

                if is_sky {
                    let depth_row = unsafe { depth_ptr.add(y * depth_stride) };
                    let sky_r = (y as f32 * sky.v_scale + sky.pitch_offset) as i32;
                    let sky_combined = &sky.extended;
                    let sky_tex_height = sky.tex_height;
                    let sky_w = sky.tex_width;
                    let y_contrib = inv_w_origin + inv_w_step_y * y as f32;
                    let mut inv_w = y_contrib + inv_w_step_x * x_start as f32;
                    let row_start = y * pitch;
                    for x in x_start..x_end {
                        unsafe {
                            let dp = depth_row.add(x);
                            let old = *dp;
                            if inv_w > old {
                                *dp = inv_w;
                                let sky_col = (sky.x_offset + x as f32 * sky.x_step)
                                    .rem_euclid(sky_w as f32)
                                    as usize;
                                if let Some(color) =
                                    sample_sky_pixel(sky_col, sky_r, sky_tex_height, sky_combined)
                                {
                                    let px = row_start + x * SOFT_PIXEL_CHANNELS;
                                    buf[px] = color[0];
                                    buf[px + 1] = color[1];
                                    buf[px + 2] = color[2];
                                    buf[px + 3] = 255;
                                }
                                if old == -1.0 {
                                    let ti = (y / TILE_SIZE) * tiles_x + (x / TILE_SIZE);
                                    let tp = tile_min_ptr.add(ti);
                                    if inv_w < *tp {
                                        *tp = inv_w;
                                    }
                                    *tile_covered_ptr.add(ti) += 1;
                                }
                            }
                        }
                        inv_w += inv_w_step_x;
                    }
                    span_ptr = span.next;
                    continue;
                }

                let mut interp_state = interpolator.init_scanline(x_start as f32, y as f32);
                let y_contrib = inv_w_origin + inv_w_step_y * y as f32;
                let mut inv_w = y_contrib + inv_w_step_x * x_start as f32;
                let row_start = y * pitch;
                let depth_row = unsafe { depth_ptr.add(y * depth_stride) };

                for x in x_start..x_end {
                    let (u, v) = interp_state.get_current_uv();
                    let colourmap = pic_data.base_colourmap(brightness, inv_w * LIGHT_SCALE);
                    let color = texture_sampler.sample(u, v, colourmap, pic_data);

                    let px = row_start + x * SOFT_PIXEL_CHANNELS;
                    buf[px] = color[0];
                    buf[px + 1] = color[1];
                    buf[px + 2] = color[2];
                    buf[px + 3] = 255;

                    unsafe {
                        let dp = depth_row.add(x);
                        let old = *dp;
                        *dp = inv_w;
                        if old == -1.0 {
                            let ti = (y / TILE_SIZE) * tiles_x + (x / TILE_SIZE);
                            let tp = tile_min_ptr.add(ti);
                            if inv_w < *tp {
                                *tp = inv_w;
                            }
                            *tile_covered_ptr.add(ti) += 1;
                        }
                    }

                    interp_state.step_x();
                    inv_w += inv_w_step_x;
                }

                span_ptr = span.next;
            }
        }
    }
}

impl Software3D {
    /// Clip, project, and emit a polygon's edges into the edge-span system.
    /// Returns true if the polygon was emitted (not masked).
    fn prepare_and_emit_polygon(
        &mut self,
        polygon: &SurfacePolygon,
        bsp3d: &BSP3D,
        sectors: &[Sector],
        pic_data: &PicData,
        player_light: usize,
    ) -> bool {
        // Skip masked walls — they need the depth buffer and are drawn in a
        // separate post-pass.
        let is_masked = matches!(
            &polygon.surface_kind,
            SurfaceKind::Vertical {
                two_sided: true,
                wall_type: WallType::Middle,
                ..
            }
        );
        if is_masked {
            return false;
        }

        // Same pipeline as render_surface_polygon up to edge emission
        self.screen_vertices_len = 0;
        self.tex_coords_len = 0;
        self.inv_w_len = 0;
        self.clipped_vertices_len = 0;

        let vert_count = polygon.vertices.len();
        if vert_count < 3 || vert_count > MAX_CLIPPED_VERTICES {
            return false;
        }
        let mut input_vertices = [Vec4::ZERO; MAX_CLIPPED_VERTICES];
        let mut input_tex_coords = [Vec3::ZERO; MAX_CLIPPED_VERTICES];

        let wall_z_range = match &polygon.surface_kind {
            SurfaceKind::Vertical {
                texture: Some(_),
                ..
            } => polygon.vertices.iter().fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(min_z, max_z), &v| {
                    let z = bsp3d.vertex_get(v).z;
                    (min_z.min(z), max_z.max(z))
                },
            ),
            _ => (0.0, 0.0),
        };

        for (i, &vertex_idx) in polygon.vertices.iter().enumerate() {
            let (_, clip_pos) = self.get_transformed_vertex(vertex_idx, bsp3d);
            let vertex = bsp3d.vertex_get(vertex_idx);
            let (u, v) = self.calculate_tex_coords(vertex, polygon, bsp3d, pic_data, wall_z_range);

            input_vertices[i] = clip_pos;
            input_tex_coords[i] = Vec3::new(u, v, clip_pos.w);
        }

        self.clip_polygon_frustum(&input_vertices, &input_tex_coords, vert_count);

        // Project to screen space
        let w_f32 = self.width as f32;
        let h_f32 = self.height as f32;
        let mut scr_min_x = f32::MAX;
        let mut scr_min_y = f32::MAX;
        let mut scr_max_x = f32::MIN;
        let mut scr_max_y = f32::MIN;

        for i in 0..self.clipped_vertices_len {
            let clip_pos = self.clipped_vertices_buffer[i];
            let tex_coord = self.clipped_tex_coords_buffer[i];

            if clip_pos.w > 0.0 {
                let inv_w = 1.0 / clip_pos.w;
                let half_w = 0.5 * w_f32;
                let half_h = 0.5 * h_f32;
                let mut screen_x = (clip_pos.x + clip_pos.w) * half_w * inv_w;
                let mut screen_y = (clip_pos.w - clip_pos.y) * half_h * inv_w;

                const SNAP: f32 = 0.01;
                if screen_x.abs() < SNAP {
                    screen_x = 0.0;
                } else if (screen_x - w_f32).abs() < SNAP {
                    screen_x = w_f32;
                }
                if screen_y.abs() < SNAP {
                    screen_y = 0.0;
                } else if (screen_y - h_f32).abs() < SNAP {
                    screen_y = h_f32;
                }

                if screen_x < scr_min_x {
                    scr_min_x = screen_x;
                }
                if screen_x > scr_max_x {
                    scr_max_x = screen_x;
                }
                if screen_y < scr_min_y {
                    scr_min_y = screen_y;
                }
                if screen_y > scr_max_y {
                    scr_max_y = screen_y;
                }

                self.screen_vertices_buffer[self.screen_vertices_len] =
                    Vec2::new(screen_x, screen_y);
                self.tex_coords_buffer[self.tex_coords_len] =
                    Vec2::new(tex_coord.x * inv_w, tex_coord.y * inv_w);
                self.inv_w_buffer[self.inv_w_len] = inv_w;

                self.screen_vertices_len += 1;
                self.tex_coords_len += 1;
                self.inv_w_len += 1;
            }
        }

        if self.screen_vertices_len < 3 {
            return false;
        }

        // Sub-pixel cull
        if (scr_max_x - scr_min_x) < 1.0 && (scr_max_y - scr_min_y) < 1.0 {
            return false;
        }

        // Screen-space backface cull: reject polygons with zero or positive
        // signed area (CW winding produces negative area in screen space).
        // Catches edge-on slivers and polygons that flip after frustum clipping.
        let mut signed_area = 0.0f32;
        for i in 0..self.screen_vertices_len {
            let j = (i + 1) % self.screen_vertices_len;
            let vi = self.screen_vertices_buffer[i];
            let vj = self.screen_vertices_buffer[j];
            signed_area += vi.x * vj.y - vj.x * vi.y;
        }
        if signed_area >= 0.0 {
            return false;
        }

        // Build the triangle interpolator
        let screen_verts = &self.screen_vertices_buffer[..self.screen_vertices_len];
        let tex_coords = &self.tex_coords_buffer[..self.tex_coords_len];
        let inv_w_slice = &self.inv_w_buffer[..self.inv_w_len];

        let interpolator = match TriangleInterpolator::new(screen_verts, tex_coords, inv_w_slice) {
            Some(interp) => interp,
            None => return false,
        };

        let is_sky = match &polygon.surface_kind {
            SurfaceKind::Vertical {
                texture: Some(tex_id),
                ..
            } => *tex_id == pic_data.sky_pic(),
            SurfaceKind::Horizontal {
                texture,
                ..
            } => *texture == pic_data.sky_num(),
            _ => false,
        };

        let brightness = ((sectors[polygon.sector_id].lightlevel >> 4) + player_light).min(15);

        self.edge_state.emit_polygon(
            screen_verts,
            inv_w_slice,
            polygon as *const _,
            interpolator,
            brightness,
            is_sky,
        );

        true
    }

    /// Front-to-back BSP traversal that emits edges into the edge-span system.
    pub(crate) fn emit_edges_bsp(
        &mut self,
        node_id: u32,
        bsp3d: &BSP3D,
        pvs: &impl PvsData,
        use_pvs: bool,
        sectors: &[Sector],
        player_pos: Vec3,
        player_light: usize,
        player_subsector_id: usize,
        pic_data: &PicData,
    ) {
        if is_subsector(node_id) {
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                subsector_index(node_id)
            };

            self.stats.subsectors_total += 1;
            if use_pvs && !pvs.is_visible(player_subsector_id, subsector_id) {
                return;
            }
            self.stats.subsectors_pvs_passed += 1;

            let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) else {
                return;
            };
            if self.is_bbox_outside_fov(&leaf.aabb) {
                return;
            }

            for poly_surface in &leaf.polygons {
                let sid = poly_surface.sector_id;
                if !self.seen_sectors[sid] {
                    self.seen_sectors[sid] = true;
                    self.visible_sectors
                        .push((sid, sectors[sid].lightlevel >> 4));
                }
                if poly_surface.is_facing_point(player_pos, &bsp3d.vertices) {
                    if self.cull_polygon_bounds(poly_surface, bsp3d).is_some() {
                        if !self.prepare_and_emit_polygon(
                            poly_surface,
                            bsp3d,
                            sectors,
                            pic_data,
                            player_light,
                        ) {
                            // Masked wall — collect for post-pass
                            if let Some(depth) = self.cull_polygon_bounds(poly_surface, bsp3d) {
                                self.visible_polygons
                                    .push((poly_surface as *const _, depth));
                            }
                        }
                    }
                }
            }
            return;
        }

        let Some(node) = bsp3d.nodes().get(node_id as usize) else {
            return;
        };
        if self.is_bbox_outside_fov(&node.aabb) {
            return;
        }

        let (front, back) = node.front_back_children(Vec2::new(player_pos.x, player_pos.y));
        self.emit_edges_bsp(
            front,
            bsp3d,
            pvs,
            use_pvs,
            sectors,
            player_pos,
            player_light,
            player_subsector_id,
            pic_data,
        );
        self.emit_edges_bsp(
            back,
            bsp3d,
            pvs,
            use_pvs,
            sectors,
            player_pos,
            player_light,
            player_subsector_id,
            pic_data,
        );
    }

    /// Headless edge-span render entry point for benchmarks. Runs the full
    /// emit → process_scanlines → draw_spans pipeline without PVS.
    #[cfg(feature = "bench")]
    pub fn draw_view_bench_edge_spans(
        &mut self,
        pos: Vec3,
        angle_rad: f32,
        pitch_rad: f32,
        subsector_id: usize,
        map_data: &MapData,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        let MapData {
            sectors,
            bsp_3d,
            pvs,
            ..
        } = map_data;

        self.prepare_vertex_cache(bsp_3d);
        self.current_frame_id = self.current_frame_id.wrapping_add(1);

        let forward = Vec3::new(
            angle_rad.cos() * pitch_rad.cos(),
            angle_rad.sin() * pitch_rad.cos(),
            pitch_rad.sin(),
        );
        self.camera_pos = pos;
        self.view_matrix = Mat4::look_at_rh(Vec3::ZERO, forward, Vec3::Z);

        self.stats.reset();
        if cfg!(feature = "hiz_prev_frame") {
            self.depth_buffer.soft_reset();
        } else {
            self.depth_buffer.reset();
        }

        self.seen_sectors.resize(sectors.len(), false);
        self.seen_sectors.fill(false);
        self.visible_sectors.clear();
        self.visible_polygons.clear();

        self.update_sky_params(angle_rad, pitch_rad, pic_data);

        self.edge_state.reset();
        self.emit_edges_bsp(
            bsp_3d.root_node(),
            bsp_3d,
            pvs,
            false,
            sectors,
            pos,
            0,
            subsector_id,
            pic_data,
        );
        let depth_ptr = self.depth_buffer.depths_raw_ptr();
        let depth_stride = self.depth_buffer.width();
        let tile_min_ptr = self.depth_buffer.tile_min_ptr();
        let tile_covered_ptr = self.depth_buffer.tile_covered_ptr();
        let tiles_x = self.depth_buffer.tiles_x();
        self.edge_state.process_and_draw_spans(
            pic_data,
            buffer,
            depth_ptr,
            depth_stride,
            tile_min_ptr,
            tile_covered_ptr,
            tiles_x,
            &self.sky,
        );
    }
}
