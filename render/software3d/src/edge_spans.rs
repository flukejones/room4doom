#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::{PicData, SurfacePolygon};
use glam::Vec2;
use render_trait::{DrawBuffer, SOFT_PIXEL_CHANNELS};

use std::alloc::{self, Layout};

use crate::render::{TextureSampler, TriangleInterpolator};

const SURF_NONE: usize = usize::MAX;
const SPAN_END: usize = usize::MAX;
const EDGE_NONE: usize = usize::MAX;

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

    /// Push an element, returning its index. No capacity check — caller must
    /// ensure the pool was sized correctly.
    #[inline(always)]
    unsafe fn push_unchecked(&mut self, val: T) -> usize {
        let idx = self.len;
        debug_assert!(idx < self.capacity);
        unsafe { self.ptr.add(idx).write(val) };
        self.len = idx + 1;
        idx
    }

    /// Raw pointer to the backing buffer.
    #[inline(always)]
    fn as_ptr(&self) -> *const T {
        self.ptr
    }

    /// Raw mutable pointer to the backing buffer.
    #[inline(always)]
    fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr
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
    /// Previous surface in the surface stack (`SURF_NONE` = head/not linked).
    prev: usize,
    /// Next surface in the surface stack (`SURF_NONE` = tail/not linked).
    next: usize,
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
    edges: BumpPool<Edge>,
    /// Surface pool.
    surfaces: BumpPool<SpanSurface>,
    /// Span pool.
    spans: BumpPool<Span>,
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
    /// Head of the intrusive surface stack (lowest key = closest).
    /// `SURF_NONE` when empty.
    surf_stack_head: usize,
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
            edges: BumpPool::new(8192),
            surfaces: BumpPool::new(4096),
            spans: BumpPool::new(16384),
            polygon_data: Vec::with_capacity(512),
            new_edges: vec![EDGE_NONE; h],
            remove_edges: vec![EDGE_NONE; h],
            active_count: 0,
            surf_stack_head: SURF_NONE,
            stack_depth: 0,
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
        self.surf_stack_head = SURF_NONE;
        self.stack_depth = 0;
        self.next_key = 0;
        self.active_count = 0;
        self.stats.reset();

        // Sentinel 0: left guard (x = -1)
        unsafe {
            self.edges.push_unchecked(Edge {
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
        }
        // Sentinel 1: right guard (x = infinity, catches all edges)
        unsafe {
            self.edges.push_unchecked(Edge {
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

        unsafe {
            self.surfaces.push_unchecked(SpanSurface {
                key: self.next_key,
                polygon_idx,
                last_u: 0,
                span_head: SPAN_END,
                spanstate: 0,
                last_inv_w: 0.0,
                prev: SURF_NONE,
                next: SURF_NONE,
            });
        }
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

            let edge_idx = unsafe {
                self.edges.push_unchecked(Edge {
                    x,
                    x_step,
                    surfs,
                    inv_w: edge_inv_w,
                    inv_w_step,
                    prev: EDGE_NONE,
                    next: EDGE_NONE,
                    next_new: EDGE_NONE,
                    next_remove: EDGE_NONE,
                })
            };
            // Insertion-sort into per-scanline new-edge chain by x.
            // edges_ptr may be stale after grow but grow only happens once
            // at the top before any pushes.
            let edges_ptr = self.edges.as_mut_ptr();
            let mut cursor = self.new_edges[y_start];
            let mut prev_cursor = EDGE_NONE;
            while cursor != EDGE_NONE {
                let cx = unsafe { (*edges_ptr.add(cursor)).x };
                if cx > x {
                    break;
                }
                prev_cursor = cursor;
                cursor = unsafe { (*edges_ptr.add(cursor)).next_new };
            }
            unsafe { (*edges_ptr.add(edge_idx)).next_new = cursor };
            if prev_cursor != EDGE_NONE {
                unsafe { (*edges_ptr.add(prev_cursor)).next_new = edge_idx };
            } else {
                self.new_edges[y_start] = edge_idx;
            }

            // Link into per-scanline removal chain
            let remove_y = y_end - 1;
            if remove_y < self.height {
                unsafe {
                    (*edges_ptr.add(edge_idx)).next_remove = self.remove_edges[remove_y];
                }
                self.remove_edges[remove_y] = edge_idx;
            }

            self.stats.edges_emitted += 1;
        }
    }

    /// Sorted merge: splice a pre-sorted new-edge chain into the AET.
    /// Both lists are sorted by x, so this is a single left-to-right pass.
    ///
    /// SAFETY: all edge indices in the new-edge chain and the AET must be
    /// valid indices into `self.edges`.
    fn aet_merge_new_edges(&mut self, mut new_idx: usize) {
        let edges = self.edges.as_mut_ptr();
        let mut cursor = unsafe { (*edges.add(AET_HEAD)).next };
        while new_idx != EDGE_NONE {
            let ne = unsafe { &*edges.add(new_idx) };
            let next_new = ne.next_new;
            let new_x = ne.x;
            // Advance AET cursor past edges with x <= new_x
            while unsafe { (*edges.add(cursor)).x } <= new_x {
                cursor = unsafe { (*edges.add(cursor)).next };
            }
            let prev = unsafe { (*edges.add(cursor)).prev };
            unsafe {
                (*edges.add(new_idx)).prev = prev;
                (*edges.add(new_idx)).next = cursor;
                (*edges.add(prev)).next = new_idx;
                (*edges.add(cursor)).prev = new_idx;
            }
            self.active_count += 1;
            new_idx = next_new;
        }
    }

    /// Remove an edge from the AET. O(1) unlink via prev/next.
    ///
    /// SAFETY: `edge_idx` must be a valid index into `self.edges`.
    #[inline(always)]
    unsafe fn aet_remove(&mut self, edge_idx: usize) {
        unsafe {
            let edges = self.edges.as_mut_ptr();
            let prev = (*edges.add(edge_idx)).prev;
            let next = (*edges.add(edge_idx)).next;
            (*edges.add(prev)).next = next;
            (*edges.add(next)).prev = prev;
        }
        self.active_count -= 1;
    }

    /// Step all active edges to the next scanline and fix sort order.
    /// Combined step + resort in one AET pass. Edges rarely cross between
    /// scanlines, so the backward scan is almost always 0–1 hops.
    ///
    /// SAFETY: AET linked list must be well-formed with valid sentinel
    /// indices 0 and 1.
    fn aet_step_and_resort(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_resort");
        let edges = self.edges.as_mut_ptr();
        let mut idx = unsafe { (*edges.add(AET_HEAD)).next };
        while idx != AET_TAIL {
            let e = unsafe { &mut *edges.add(idx) };
            e.x += e.x_step;
            e.inv_w += e.inv_w_step;
            let next = e.next;
            let prev = e.prev;
            let new_x = e.x;
            if unsafe { (*edges.add(prev)).x } > new_x {
                // Unlink
                unsafe {
                    (*edges.add(prev)).next = next;
                    (*edges.add(next)).prev = prev;
                }
                // Walk left; left sentinel (x=-1) guarantees termination
                let mut ia = unsafe { (*edges.add(prev)).prev };
                while unsafe { (*edges.add(ia)).x } > new_x {
                    ia = unsafe { (*edges.add(ia)).prev };
                }
                let ib = unsafe { (*edges.add(ia)).next };
                unsafe {
                    (*edges.add(idx)).prev = ia;
                    (*edges.add(idx)).next = ib;
                    (*edges.add(ia)).next = idx;
                    (*edges.add(ib)).prev = idx;
                }
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
            let new_head = unsafe { *self.new_edges.get_unchecked(y) };
            if new_head != EDGE_NONE {
                self.aet_merge_new_edges(new_head);
            }

            if self.active_count > self.stats.max_active_edges {
                self.stats.max_active_edges = self.active_count;
            }

            self.generate_spans(y);

            let mut remove_idx = unsafe { *self.remove_edges.get_unchecked(y) };
            while remove_idx != EDGE_NONE {
                let next_remove = unsafe { (*self.edges.as_ptr().add(remove_idx)).next_remove };
                unsafe { self.aet_remove(remove_idx) };
                remove_idx = next_remove;
            }

            self.aet_step_and_resort();
        }
    }

    /// Walk the AET left-to-right via the intrusive linked list, using the
    /// surface stack to determine visibility and emit spans.
    ///
    /// SAFETY: AET linked list and surface stack must be well-formed.
    fn generate_spans(&mut self, y: usize) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_gen");
        let edges = self.edges.as_ptr();
        let w_max = self.w_max;
        let mut idx = unsafe { (*edges.add(AET_HEAD)).next };
        while idx != AET_TAIL {
            let edge = unsafe { &*edges.add(idx) };
            let edge_inv_w = edge.inv_w;
            let surfs = edge.surfs;
            let next = edge.next;
            // Clamp once, reuse for both leading and trailing
            let ix = edge.x.max(0.0).min(w_max) as i32;

            if surfs[0] != SURF_NONE {
                self.trailing_edge(surfs[0], ix, edge_inv_w, y);
            }

            if surfs[1] != SURF_NONE {
                self.leading_edge(surfs[1], ix, edge_inv_w, y);
            }

            idx = next;
        }
    }

    /// Handle a leading edge: surface is entering the active region.
    /// Inserts into intrusive surface stack sorted by key (ascending).
    ///
    /// SAFETY: `surf_idx` must be a valid index into `self.surfaces`.
    #[inline(always)]
    fn leading_edge(&mut self, surf_idx: usize, ix: i32, edge_inv_w: f32, y: usize) {
        let surfs = self.surfaces.as_mut_ptr();
        let s = unsafe { &mut *surfs.add(surf_idx) };
        s.spanstate += 1;
        if s.spanstate != 1 {
            return;
        }

        let surf_key = s.key;
        let head = self.surf_stack_head;

        if head == SURF_NONE {
            // Empty stack — just insert
            self.surf_stack_head = surf_idx;
            unsafe {
                (*surfs.add(surf_idx)).prev = SURF_NONE;
                (*surfs.add(surf_idx)).next = SURF_NONE;
                (*surfs.add(surf_idx)).last_u = ix;
                (*surfs.add(surf_idx)).last_inv_w = edge_inv_w;
            }
            self.stack_depth = 1;
            if self.stack_depth > self.stats.max_stack_depth {
                self.stats.max_stack_depth = self.stack_depth;
            }
            return;
        }

        let head_key = unsafe { (*surfs.add(head)).key };

        if surf_key < head_key {
            // New surface goes in front — emit span for old head
            let old_last_u = unsafe { (*surfs.add(head)).last_u };
            let old_inv_w = unsafe { (*surfs.add(head)).last_inv_w };
            if ix > old_last_u {
                self.emit_span(head, old_last_u, ix, old_inv_w, edge_inv_w, y);
            }
            // Insert at head
            unsafe {
                (*surfs.add(surf_idx)).prev = SURF_NONE;
                (*surfs.add(surf_idx)).next = head;
                (*surfs.add(head)).prev = surf_idx;
                (*surfs.add(surf_idx)).last_u = ix;
                (*surfs.add(surf_idx)).last_inv_w = edge_inv_w;
            }
            self.surf_stack_head = surf_idx;
        } else {
            // Walk to find insertion point (common case: goes behind head)
            let mut prev_cursor = head;
            let mut cursor = unsafe { (*surfs.add(head)).next };
            while cursor != SURF_NONE {
                let ck = unsafe { (*surfs.add(cursor)).key };
                if ck > surf_key {
                    break;
                }
                prev_cursor = cursor;
                cursor = unsafe { (*surfs.add(cursor)).next };
            }
            // Insert after prev_cursor
            unsafe {
                (*surfs.add(surf_idx)).prev = prev_cursor;
                (*surfs.add(surf_idx)).next = cursor;
                (*surfs.add(prev_cursor)).next = surf_idx;
                (*surfs.add(surf_idx)).last_u = ix;
                (*surfs.add(surf_idx)).last_inv_w = edge_inv_w;
            }
            if cursor != SURF_NONE {
                unsafe { (*surfs.add(cursor)).prev = surf_idx };
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
    /// SAFETY: `surf_idx` must be a valid index into `self.surfaces`.
    #[inline(always)]
    fn trailing_edge(&mut self, surf_idx: usize, ix: i32, edge_inv_w: f32, y: usize) {
        let surfs = self.surfaces.as_mut_ptr();
        let s = unsafe { &mut *surfs.add(surf_idx) };
        s.spanstate -= 1;
        if s.spanstate != 0 {
            return;
        }

        if self.surf_stack_head == surf_idx {
            let last_u = s.last_u;
            let last_inv_w = s.last_inv_w;
            if ix > last_u {
                self.emit_span(surf_idx, last_u, ix, last_inv_w, edge_inv_w, y);
            }
            let new_head = s.next;
            if new_head != SURF_NONE {
                unsafe {
                    (*surfs.add(new_head)).last_u = ix;
                    (*surfs.add(new_head)).last_inv_w = edge_inv_w;
                }
            }
        }

        // Unlink
        let prev = unsafe { (*surfs.add(surf_idx)).prev };
        let next = unsafe { (*surfs.add(surf_idx)).next };
        if prev != SURF_NONE {
            unsafe { (*surfs.add(prev)).next = next };
        } else {
            self.surf_stack_head = next;
        }
        if next != SURF_NONE {
            unsafe { (*surfs.add(next)).prev = prev };
        }
        unsafe {
            (*surfs.add(surf_idx)).prev = SURF_NONE;
            (*surfs.add(surf_idx)).next = SURF_NONE;
        }
        self.stack_depth -= 1;
    }

    /// Emit a span for a surface. Uses pre-reserved capacity to avoid
    /// per-push bounds checks.
    ///
    /// SAFETY: `surf_idx` must be a valid index into `self.surfaces`.
    /// Caller must ensure `self.spans` has spare capacity.
    #[inline(always)]
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

        let old_head = unsafe { (*self.surfaces.as_ptr().add(surf_idx)).span_head };

        let span_idx = unsafe {
            self.spans.push_unchecked(Span {
                x_start: xs,
                x_end: xe,
                y,
                inv_w_start,
                inv_w_end,
                next: old_head,
            })
        };

        unsafe { (*self.surfaces.as_mut_ptr().add(surf_idx)).span_head = span_idx };
        self.stats.spans_generated += 1;
    }

    /// Draw all accumulated spans. For each surface, walks its span list and
    /// paints pixels using the polygon's interpolation data.
    pub fn draw_spans(&self, pic_data: &mut PicData, buffer: &mut impl DrawBuffer) {
        #[cfg(feature = "hprof")]
        profile!("edge_spans_draw");
        let sky_pic = pic_data.sky_pic();
        let sky_num = pic_data.sky_num();
        let pitch = buffer.pitch();
        let buf = buffer.buf_mut();

        let spans_ptr = self.spans.as_ptr();
        for surf in self.surfaces.iter() {
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
                let span = unsafe { &*spans_ptr.add(span_idx) };
                let y = span.y;
                let x_start = span.x_start;
                let x_end = span.x_end;
                let span_width = x_end - x_start;

                if span_width == 0 || is_sky {
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
                let row_start = y * pitch;

                for x in x_start..x_end {
                    let (u, v) = interp_state.get_current_uv();
                    let colourmap = pic_data.base_colourmap(brightness, edge_inv_w * LIGHT_SCALE);
                    let color = texture_sampler.sample(u, v, colourmap, pic_data);

                    let px = row_start + x * SOFT_PIXEL_CHANNELS;
                    buf[px] = color[0];
                    buf[px + 1] = color[1];
                    buf[px + 2] = color[2];
                    buf[px + 3] = 255;

                    interp_state.step_x();
                    edge_inv_w += inv_w_dx;
                }

                span_idx = span.next;
            }
        }
    }
}
