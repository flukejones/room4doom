//! Many helper functions related to traversing the map, crossing or finding
//! lines.

use crate::thing::{MapObject, PT_ADDLINES, PT_ADDTHINGS, PT_EARLYOUT};

use crate::level::LevelState;
use level::MapPtr;
use level::level_data::LevelData;
use level::map_defs::{LineDef, Node, SlopeType, is_subsector, subsector_index};
#[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
use math::fixed_point::{FRACBITS, WideInner};
use math::{DivLineFixed, FixedT, intercept_vector_fixed, point_on_divline_side};

/// OG Doom `P_PointOnLineSide` — fixed-point side test.
///
/// Takes raw fixed-point x/y and linedef, matches OG Doom's
/// `FixedMul(line->dy >> FRACBITS, dx)` vs `FixedMul(dy, line->dx >>
/// FRACBITS)`.
#[inline]
pub fn point_on_line_side(x: FixedT, y: FixedT, line: &LineDef) -> usize {
    let ldx = FixedT::from_fixed(line.delta_fp[0]);
    let ldy = FixedT::from_fixed(line.delta_fp[1]);
    let v1x = FixedT::from_fixed(line.v1.x_fp.to_fixed_raw());
    let v1y = FixedT::from_fixed(line.v1.y_fp.to_fixed_raw());

    let zero = FixedT::ZERO;
    if ldx == zero {
        return if x <= v1x {
            (ldy > zero) as usize
        } else {
            (ldy < zero) as usize
        };
    }
    if ldy == zero {
        return if y <= v1y {
            (ldx < zero) as usize
        } else {
            (ldx > zero) as usize
        };
    }

    // OG: left = FixedMul(line->dy >> FRACBITS, dx)
    //     right = FixedMul(dy, line->dx >> FRACBITS)
    #[cfg(not(any(feature = "fixed64", feature = "fixed64hd")))]
    {
        let dx = x.to_fixed_raw().wrapping_sub(v1x.to_fixed_raw());
        let dy = y.to_fixed_raw().wrapping_sub(v1y.to_fixed_raw());
        let left = ((ldy.to_fixed_raw() >> 16) as i64 * dx as i64) >> 16;
        let right = (dy as i64 * (ldx.to_fixed_raw() >> 16) as i64) >> 16;
        if right < left { 0 } else { 1 }
    }
    #[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
    {
        let dx = x.raw().wrapping_sub(v1x.raw());
        let dy = y.raw().wrapping_sub(v1y.raw());
        let left = (ldy.raw() >> FRACBITS) as WideInner * dx as WideInner;
        let right = dy as WideInner * (ldx.raw() >> FRACBITS) as WideInner;
        if right < left { 0 } else { 1 }
    }
}

/// OG `P_BoxOnLineSide` — test bbox against linedef using fixed-point.
/// bbox layout: [BOXTOP, BOXBOTTOM, BOXLEFT, BOXRIGHT] in raw 16.16.
/// Returns -1 if the line runs through the box at all.
#[inline]
pub fn box_on_line_side(tmbox: &[i32; 4], ld: &LineDef) -> i32 {
    let mut p1;
    let mut p2;
    let ldx = ld.delta_fp[0];
    let ldy = ld.delta_fp[1];

    match ld.slopetype {
        SlopeType::Horizontal => {
            let v1y = ld.v1.y_fp.to_fixed_raw();
            p1 = (tmbox[BOXTOP] > v1y) as i32;
            p2 = (tmbox[BOXBOTTOM] > v1y) as i32;
            if ldx < 0 {
                p1 ^= 1;
                p2 ^= 1;
            }
        }
        SlopeType::Vertical => {
            let v1x = ld.v1.x_fp.to_fixed_raw();
            p1 = (tmbox[BOXRIGHT] < v1x) as i32;
            p2 = (tmbox[BOXLEFT] < v1x) as i32;
            if ldy < 0 {
                p1 ^= 1;
                p2 ^= 1;
            }
        }
        SlopeType::Positive => {
            p1 = point_on_line_side(
                FixedT::from_fixed(tmbox[BOXLEFT]),
                FixedT::from_fixed(tmbox[BOXTOP]),
                ld,
            ) as i32;
            p2 = point_on_line_side(
                FixedT::from_fixed(tmbox[BOXRIGHT]),
                FixedT::from_fixed(tmbox[BOXBOTTOM]),
                ld,
            ) as i32;
        }
        SlopeType::Negative => {
            p1 = point_on_line_side(
                FixedT::from_fixed(tmbox[BOXRIGHT]),
                FixedT::from_fixed(tmbox[BOXTOP]),
                ld,
            ) as i32;
            p2 = point_on_line_side(
                FixedT::from_fixed(tmbox[BOXLEFT]),
                FixedT::from_fixed(tmbox[BOXBOTTOM]),
                ld,
            ) as i32;
        }
    }

    if p1 == p2 {
        return p1;
    }

    -1
}

const BOXTOP: usize = 0;
const BOXBOTTOM: usize = 1;
const BOXLEFT: usize = 2;
const BOXRIGHT: usize = 3;

/// OG Doom `P_DivlineSide` — fixed-point side test.
/// Returns 0 (front), 1 (back), or 2 (exactly on the partition line).
#[inline]
pub fn p_divline_side_raw(
    x: FixedT,
    y: FixedT,
    nx: FixedT,
    ny: FixedT,
    ndx: FixedT,
    ndy: FixedT,
) -> usize {
    let zero = FixedT::ZERO;
    if ndx == zero {
        if x == nx {
            return 2;
        }
        return if x <= nx {
            (ndy > zero) as usize
        } else {
            (ndy < zero) as usize
        };
    }
    if ndy == zero {
        if y == ny {
            return 2;
        }
        return if y <= ny {
            (ndx < zero) as usize
        } else {
            (ndx > zero) as usize
        };
    }

    // OG: left = (node->dy >> FRACBITS) * (dx >> FRACBITS)
    //     right = (dy >> FRACBITS) * (node->dx >> FRACBITS)
    #[cfg(not(any(feature = "fixed64", feature = "fixed64hd")))]
    let (left, right) = {
        let dx = x.to_fixed_raw().wrapping_sub(nx.to_fixed_raw());
        let dy = y.to_fixed_raw().wrapping_sub(ny.to_fixed_raw());
        (
            ((ndy.to_fixed_raw() >> 16) as i64) * ((dx >> 16) as i64),
            ((dy >> 16) as i64) * ((ndx.to_fixed_raw() >> 16) as i64),
        )
    };
    #[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
    let (left, right) = {
        let dx = x.raw().wrapping_sub(nx.raw());
        let dy = y.raw().wrapping_sub(ny.raw());
        (
            (ndy.raw() >> FRACBITS) as WideInner * (dx >> FRACBITS) as WideInner,
            (dy >> FRACBITS) as WideInner * (ndx.raw() >> FRACBITS) as WideInner,
        )
    };

    if right < left {
        0
    } else if left == right {
        2
    } else {
        1
    }
}

/// Wrapper for BSP node side test (converts Node f32 fields to fixed).
#[inline]
fn p_divline_side(x: FixedT, y: FixedT, node: &Node) -> usize {
    p_divline_side_raw(
        x,
        y,
        FixedT::from_f32(node.xy.x),
        FixedT::from_f32(node.xy.y),
        FixedT::from_f32(node.delta.x),
        FixedT::from_f32(node.delta.y),
    )
}

/// BSP traversal trace using fixed-point coordinates matching OG Doom's
/// `R_PointOnSide`. Finds which subsectors a line or radius query intersects.
pub struct BSPTrace {
    pub origin_x: FixedT,
    pub origin_y: FixedT,
    pub endpoint_x: FixedT,
    pub endpoint_y: FixedT,
    pub nodes: Vec<u32>,
}

impl BSPTrace {
    #[inline]
    pub fn new_line(
        origin_x: FixedT,
        origin_y: FixedT,
        endpoint_x: FixedT,
        endpoint_y: FixedT,
        radius: FixedT,
    ) -> Self {
        let dx = endpoint_x - origin_x;
        let dy = endpoint_y - origin_y;
        let fwd_bam = math::r_point_to_angle(dx, dy);
        let back_bam = fwd_bam.wrapping_add(math::ANG180);

        let offset = |bam: u32| -> (FixedT, FixedT) {
            (radius * math::fine_cos(bam), radius * math::fine_sin(bam))
        };

        let (fwd_dx, fwd_dy) = offset(fwd_bam);
        let (back_dx, back_dy) = offset(back_bam);

        Self {
            origin_x: origin_x + back_dx,
            origin_y: origin_y + back_dy,
            endpoint_x: endpoint_x + fwd_dx,
            endpoint_y: endpoint_y + fwd_dy,
            nodes: Vec::with_capacity(50),
        }
    }

    #[inline]
    pub fn find_intercepts(&mut self, node_id: u32, map: &LevelData, count: &mut u32) {
        self.find_line_inner(node_id, map, count);
    }

    /// Trace a line through the BSP from origin to endpoint using fixed-point
    /// `R_PointOnSide` for node side determination.
    #[inline]
    fn find_line_inner(&mut self, node_id: u32, map: &LevelData, count: &mut u32) {
        *count += 1;
        if is_subsector(node_id) {
            let ss_idx = subsector_index(node_id) as u32;
            if !self.nodes.contains(&ss_idx) {
                self.nodes.push(ss_idx);
            }
            return;
        }

        let node = &map.get_nodes()[node_id as usize];

        // OG P_CrossBSPNode: determine which side origin is on, cross it first,
        // then cross the other side only if the endpoint is on it.
        let mut side1 = p_divline_side(self.origin_x, self.origin_y, node);
        if side1 == 2 {
            side1 = 0; // on the partition line — treat as front, cross both
        }

        self.find_line_inner(node.children[side1], map, count);

        let side2 = p_divline_side(self.endpoint_x, self.endpoint_y, node);
        if side2 != side1 {
            self.find_line_inner(node.children[side1 ^ 1], map, count);
        }
    }
}

#[derive(Clone)]
pub struct Intercept {
    pub frac: FixedT,
    pub line: Option<MapPtr<LineDef>>,
    pub thing: Option<MapPtr<MapObject>>,
}

impl Default for Intercept {
    fn default() -> Self {
        Self {
            frac: FixedT::ZERO,
            line: None,
            thing: None,
        }
    }
}

impl PartialEq for Intercept {
    fn eq(&self, other: &Self) -> bool {
        self.frac == other.frac
    }
}

impl PartialOrd for Intercept {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Intercept {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.frac
            .partial_cmp(&other.frac)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl Eq for Intercept {}

pub struct BestSlide {
    pub best_slide_frac: FixedT,
    pub second_slide_frac: FixedT,
    pub best_slide_line: Option<MapPtr<LineDef>>,
    pub second_slide_line: Option<MapPtr<LineDef>>,
}

impl Default for BestSlide {
    fn default() -> Self {
        Self {
            best_slide_frac: FixedT::ZERO,
            second_slide_frac: FixedT::ZERO,
            best_slide_line: None,
            second_slide_line: None,
        }
    }
}

/// Functions like `P_LineOpening`
///
/// BSP boundary: sector heights (f32) are wrapped to `FixedT` at
/// construction. All consumers operate on FixedT directly.
#[derive(Debug)]
pub struct PortalZ {
    /// The lowest ceiling of the portal line
    pub top_z: FixedT,
    /// The highest floor of the portal line
    pub bottom_z: FixedT,
    /// Range between `bottom_z` and `top_z`
    pub range: FixedT,
    /// The lowest floor of the portal line
    pub lowest_z: FixedT,
}

impl Default for PortalZ {
    fn default() -> Self {
        Self {
            top_z: FixedT::ZERO,
            bottom_z: FixedT::ZERO,
            range: FixedT::ZERO,
            lowest_z: FixedT::ZERO,
        }
    }
}

impl PortalZ {
    /// BSP boundary: wraps sector f32 heights into `FixedT`
    #[inline]
    pub fn new(line: &LineDef) -> Self {
        if line.backsector.is_none() {
            return Self::default();
        }

        let front = &line.frontsector;
        let back = unsafe { line.backsector.as_ref().unwrap_unchecked() };

        let front_ceil: FixedT = FixedT::from_fixed(front.ceilingheight.to_fixed_raw());
        let back_ceil: FixedT = FixedT::from_fixed(back.ceilingheight.to_fixed_raw());
        let front_floor: FixedT = FixedT::from_fixed(front.floorheight.to_fixed_raw());
        let back_floor: FixedT = FixedT::from_fixed(back.floorheight.to_fixed_raw());

        let top_z = if front_ceil < back_ceil {
            front_ceil
        } else {
            back_ceil
        };

        let (bottom_z, lowest_z) = if front_floor > back_floor {
            (front_floor, back_floor)
        } else {
            (back_floor, front_floor)
        };

        Self {
            range: top_z - bottom_z,
            top_z,
            bottom_z,
            lowest_z,
        }
    }
}

#[inline]
/// Walk intercepts in nearest-first order up to `max_frac`, calling `trav` on
/// each. Returns false if `trav` returns false (early termination).
pub fn traverse_intercepts(
    intercepts: &mut [Intercept],
    max_frac: FixedT,
    mut trav: impl FnMut(&mut Intercept) -> bool,
) -> bool {
    if intercepts.is_empty() {
        return true;
    }
    let mut intercept: *mut Intercept = unsafe { intercepts.get_unchecked_mut(0) };
    let mut intercepts = Vec::from(intercepts);
    let mut count = intercepts.len();

    while count != 0 {
        count -= 1;
        let mut dist = FixedT::MAX;

        for i in intercepts.iter_mut() {
            if i.frac < dist {
                dist = i.frac;
                intercept = i;
            }
        }

        if dist > max_frac {
            return true;
        }

        unsafe {
            if !trav(&mut *intercept) {
                return false;
            }

            (*intercept).frac = FixedT::MAX;
        }
    }
    true
}

// --- Blockmap-based path_traverse (OG Doom P_PathTraverse) ---

const MAPBLOCKSHIFT: i32 = 23;
const MAPBTOFRAC: i32 = 7;
const MAPBLOCKSIZE: i32 = 128 << 16;
const FRACUNIT_I: i32 = 0x10000;

/// OG Doom `P_PathTraverse` — blockmap DDA ray march.
///
/// Walks blockmap cells along the ray, collecting line and thing intercepts.
/// Used by the fixed-point backend for demo-deterministic hitscan.
pub fn path_traverse_blockmap(
    origin_x: FixedT,
    origin_y: FixedT,
    endpoint_x: FixedT,
    endpoint_y: FixedT,
    flags: i32,
    level: &mut LevelState,
    trav: impl FnMut(&mut Intercept) -> bool,
) -> bool {
    let earlyout = flags & PT_EARLYOUT != 0;
    let mut intercepts: Vec<Intercept> = Vec::with_capacity(20);

    let bm = level.level_data.blockmap();
    let bmaporgx = bm.x_origin;
    let bmaporgy = bm.y_origin;
    let bmapwidth = bm.columns;
    let bmapheight = bm.rows;

    let mut x1 = origin_x.to_fixed_raw();
    let mut y1 = origin_y.to_fixed_raw();
    let mut x2 = endpoint_x.to_fixed_raw();
    let mut y2 = endpoint_y.to_fixed_raw();

    // Don't side exactly on a line
    if (x1 - bmaporgx) & (MAPBLOCKSIZE - 1) == 0 {
        x1 += FRACUNIT_I;
    }
    if (y1 - bmaporgy) & (MAPBLOCKSIZE - 1) == 0 {
        y1 += FRACUNIT_I;
    }

    let trace_fixed = DivLineFixed {
        x: FixedT::from_fixed(x1),
        y: FixedT::from_fixed(y1),
        dx: FixedT::from_fixed(x2 - x1),
        dy: FixedT::from_fixed(y2 - y1),
    };

    x1 -= bmaporgx;
    y1 -= bmaporgy;
    let xt1 = x1 >> MAPBLOCKSHIFT;
    let yt1 = y1 >> MAPBLOCKSHIFT;

    x2 -= bmaporgx;
    y2 -= bmaporgy;
    let xt2 = x2 >> MAPBLOCKSHIFT;
    let yt2 = y2 >> MAPBLOCKSHIFT;

    let mapxstep: i32;
    let mapystep: i32;
    let ystep: i32;
    let xstep: i32;
    let mut partial: i32;

    if xt2 > xt1 {
        mapxstep = 1;
        partial = FRACUNIT_I - ((x1 >> MAPBTOFRAC) & (FRACUNIT_I - 1));
        let dx_abs = (x2 - x1).abs();
        ystep = if dx_abs != 0 {
            FixedT::from_fixed(y2 - y1)
                .fixed_div(FixedT::from_fixed(dx_abs))
                .to_fixed_raw()
        } else {
            256 * FRACUNIT_I
        };
    } else if xt2 < xt1 {
        mapxstep = -1;
        partial = (x1 >> MAPBTOFRAC) & (FRACUNIT_I - 1);
        let dx_abs = (x2 - x1).abs();
        ystep = if dx_abs != 0 {
            FixedT::from_fixed(y2 - y1)
                .fixed_div(FixedT::from_fixed(dx_abs))
                .to_fixed_raw()
        } else {
            256 * FRACUNIT_I
        };
    } else {
        mapxstep = 0;
        partial = FRACUNIT_I;
        ystep = 256 * FRACUNIT_I;
    }

    let mut yintercept = (y1 >> MAPBTOFRAC)
        + FixedT::from_fixed(partial)
            .fixed_mul(FixedT::from_fixed(ystep))
            .to_fixed_raw();

    if yt2 > yt1 {
        mapystep = 1;
        partial = FRACUNIT_I - ((y1 >> MAPBTOFRAC) & (FRACUNIT_I - 1));
        let dy_abs = (y2 - y1).abs();
        xstep = if dy_abs != 0 {
            FixedT::from_fixed(x2 - x1)
                .fixed_div(FixedT::from_fixed(dy_abs))
                .to_fixed_raw()
        } else {
            256 * FRACUNIT_I
        };
    } else if yt2 < yt1 {
        mapystep = -1;
        partial = (y1 >> MAPBTOFRAC) & (FRACUNIT_I - 1);
        let dy_abs = (y2 - y1).abs();
        xstep = if dy_abs != 0 {
            FixedT::from_fixed(x2 - x1)
                .fixed_div(FixedT::from_fixed(dy_abs))
                .to_fixed_raw()
        } else {
            256 * FRACUNIT_I
        };
    } else {
        mapystep = 0;
        partial = FRACUNIT_I;
        xstep = 256 * FRACUNIT_I;
    }

    let mut xintercept = (x1 >> MAPBTOFRAC)
        + FixedT::from_fixed(partial)
            .fixed_mul(FixedT::from_fixed(xstep))
            .to_fixed_raw();

    let mut mapx = xt1;
    let mut mapy = yt1;

    level.valid_count = level.valid_count.wrapping_add(1);
    let valid_count = level.valid_count;

    for _count in 0..64 {
        if flags & PT_ADDLINES != 0
            && !block_lines_iterator(
                mapx,
                mapy,
                &level.level_data,
                &trace_fixed,
                &mut intercepts,
                earlyout,
                valid_count,
            )
        {
            return false;
        }

        if flags & PT_ADDTHINGS != 0
            && !block_things_iterator(
                mapx,
                mapy,
                bmapwidth,
                bmapheight,
                &level.blocklinks,
                &trace_fixed,
                &mut intercepts,
                valid_count,
            )
        {
            return false;
        }

        if mapx == xt2 && mapy == yt2 {
            break;
        }

        if (yintercept >> 16) == mapy {
            yintercept += ystep;
            mapx += mapxstep;
        } else if (xintercept >> 16) == mapx {
            xintercept += xstep;
            mapy += mapystep;
        }
    }

    intercepts.sort();
    traverse_intercepts(&mut intercepts, FixedT::ONE, trav)
}

/// Iterate all linedefs in blockmap cell (bx, by), adding intercepts.
fn block_lines_iterator(
    bx: i32,
    by: i32,
    level_data: &LevelData,
    trace_fixed: &DivLineFixed,
    intercepts: &mut Vec<Intercept>,
    earlyout: bool,
    valid_count: usize,
) -> bool {
    let bm = level_data.blockmap();
    if bx < 0 || by < 0 || bx >= bm.columns || by >= bm.rows {
        return true;
    }
    let idx = (by * bm.columns + bx) as usize;
    let start = bm.block_offsets[idx];
    let end = bm.block_offsets[idx + 1];

    for i in start..end {
        let mut line = bm.block_lines[i].clone();
        if line.valid_count == valid_count {
            continue;
        }
        line.valid_count = valid_count;

        if !add_line_intercepts(trace_fixed, line, intercepts, earlyout) {
            return false;
        }
    }
    true
}

/// Iterate all things in blockmap cell (bx, by), adding intercepts.
fn block_things_iterator(
    bx: i32,
    by: i32,
    bmapwidth: i32,
    bmapheight: i32,
    blocklinks: &[Option<*mut MapObject>],
    trace_fixed: &DivLineFixed,
    intercepts: &mut Vec<Intercept>,
    valid_count: usize,
) -> bool {
    if bx < 0 || by < 0 || bx >= bmapwidth || by >= bmapheight {
        return true;
    }
    let idx = (by * bmapwidth + bx) as usize;
    let mut mobj_ptr = blocklinks[idx];

    while let Some(ptr) = mobj_ptr {
        let thing = unsafe { &mut *ptr };
        if !add_thing_intercept(trace_fixed, intercepts, thing, valid_count) {
            return false;
        }
        mobj_ptr = thing.b_next;
    }
    true
}

/// OG Doom `FRACUNIT * 16` threshold for switching side-test routines.
const FRACUNIT16: FixedT = FixedT::from_fixed(16 * 0x10000);

/// Check the line and add the intercept if valid.
///
/// Matches OG Doom `PIT_AddLineIntercepts`: uses fixed-point side tests
/// and `P_InterceptVector` for the fraction computation.
#[inline]
fn add_line_intercepts(
    trace_fixed: &DivLineFixed,
    line: MapPtr<LineDef>,
    intercepts: &mut Vec<Intercept>,
    earlyout: bool,
) -> bool {
    let v1x = FixedT::from_fixed(line.v1.x_fp.to_fixed_raw());
    let v1y = FixedT::from_fixed(line.v1.y_fp.to_fixed_raw());
    let v2x = FixedT::from_fixed(line.v2.x_fp.to_fixed_raw());
    let v2y = FixedT::from_fixed(line.v2.y_fp.to_fixed_raw());

    // OG Doom: avoid precision problems with two routines
    let neg_frac16 = FixedT::ZERO - FRACUNIT16;
    let (s1, s2) = if trace_fixed.dx > FRACUNIT16
        || trace_fixed.dy > FRACUNIT16
        || trace_fixed.dx < neg_frac16
        || trace_fixed.dy < neg_frac16
    {
        (
            point_on_divline_side(v1x, v1y, trace_fixed),
            point_on_divline_side(v2x, v2y, trace_fixed),
        )
    } else {
        (
            point_on_line_side(trace_fixed.x, trace_fixed.y, &line),
            point_on_line_side(
                trace_fixed.x + trace_fixed.dx,
                trace_fixed.y + trace_fixed.dy,
                &line,
            ),
        )
    };

    if s1 == s2 {
        return true;
    }

    let dl = DivLineFixed {
        x: v1x,
        y: v1y,
        dx: FixedT::from_fixed(line.delta_fp[0]),
        dy: FixedT::from_fixed(line.delta_fp[1]),
    };
    let frac = intercept_vector_fixed(trace_fixed, &dl);

    if frac < FixedT::ZERO {
        return true;
    }

    if earlyout && frac < FixedT::ONE && line.backsector.is_none() {
        return false;
    }

    intercepts.push(Intercept {
        frac,
        line: Some(line),
        thing: None,
    });
    true
}

/// OG Doom `PIT_AddThingIntercepts` — corner-to-corner crossection test.
#[inline]
fn add_thing_intercept(
    trace_fixed: &DivLineFixed,
    intercepts: &mut Vec<Intercept>,
    thing: &mut MapObject,
    valid_count: usize,
) -> bool {
    if thing.valid_count == valid_count {
        return true;
    }
    thing.valid_count = valid_count;

    let tx = thing.x;
    let ty = thing.y;
    let tr = thing.radius;

    let tracepositive = (trace_fixed.dx.raw() ^ trace_fixed.dy.raw()) > 0;

    let (x1, y1, x2, y2) = if tracepositive {
        (tx - tr, ty + tr, tx + tr, ty - tr)
    } else {
        (tx - tr, ty - tr, tx + tr, ty + tr)
    };

    let s1 = point_on_divline_side(x1, y1, trace_fixed);
    let s2 = point_on_divline_side(x2, y2, trace_fixed);

    if s1 == s2 {
        return true;
    }

    let dl = DivLineFixed {
        x: x1,
        y: y1,
        dx: x2 - x1,
        dy: y2 - y1,
    };
    let frac = intercept_vector_fixed(trace_fixed, &dl);

    if frac < FixedT::ZERO {
        return true;
    }

    intercepts.push(Intercept {
        frac,
        line: None,
        thing: Some(MapPtr::new(thing)),
    });
    true
}
