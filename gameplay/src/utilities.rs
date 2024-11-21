//! Many helper functions related to traversing the map, crossing or finding
//! lines.

use crate::thing::{MapObject, PT_ADDLINES, PT_ADDTHINGS, PT_EARLYOUT};

use crate::level::map_data::BSPTrace;
use crate::level::map_defs::{BBox, LineDef, SlopeType};
use crate::level::Level;
use crate::MapPtr;
use glam::Vec2;
use math::{circle_seg_collide, intercept_vector, point_on_side, Trace};

/// Returns -1 if the line runs through the box at all
#[inline]
pub fn box_on_line_side(tmbox: &BBox, ld: &LineDef) -> i32 {
    let p1;
    let p2;

    match ld.slopetype {
        SlopeType::Horizontal => {
            p1 = (tmbox.top > ld.v1.y) as i32;
            p2 = (tmbox.bottom > ld.v1.y) as i32;
        }
        SlopeType::Vertical => {
            p1 = (tmbox.right > ld.v1.x) as i32;
            p2 = (tmbox.left > ld.v1.x) as i32;
        }
        SlopeType::Positive => {
            p1 = ld.point_on_side(Vec2::new(tmbox.left, tmbox.top)) as i32;
            p2 = ld.point_on_side(Vec2::new(tmbox.right, tmbox.bottom)) as i32;
        }
        SlopeType::Negative => {
            p1 = ld.point_on_side(Vec2::new(tmbox.right, tmbox.top)) as i32;
            p2 = ld.point_on_side(Vec2::new(tmbox.left, tmbox.bottom)) as i32;
        }
    }
    -1
}

#[derive(Default, Clone, PartialEq)]
pub struct Intercept {
    pub frac: f32,
    pub line: Option<MapPtr<LineDef>>,
    pub thing: Option<MapPtr<MapObject>>,
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
        if self.frac < other.frac {
            std::cmp::Ordering::Less
        } else if self.frac > other.frac {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    }
}

impl Eq for Intercept {}

#[derive(Default)]
pub struct BestSlide {
    pub best_slide_frac: f32,
    pub second_slide_frac: f32,
    pub best_slide_line: Option<MapPtr<LineDef>>,
    pub second_slide_line: Option<MapPtr<LineDef>>,
}

impl BestSlide {
    #[inline]
    pub fn new() -> Self {
        BestSlide {
            best_slide_frac: 1.0,
            ..Default::default()
        }
    }
}

/// Functions like `P_LineOpening`
#[derive(Default, Debug)]
pub struct PortalZ {
    /// The lowest ceiling of the portal line
    pub top_z: f32,
    /// The highest floor of the portal line
    pub bottom_z: f32,
    /// Range between `bottom_z` and `top_z`
    pub range: f32,
    /// The lowest floor of the portal line
    pub lowest_z: f32,
}

impl PortalZ {
    #[inline]
    pub fn new(line: &LineDef) -> Self {
        if line.backsector.is_none() {
            return Self::default();
        }

        let front = &line.frontsector;
        let back = unsafe { line.backsector.as_ref().unwrap_unchecked() };

        let mut ww = PortalZ {
            top_z: 0.0,
            bottom_z: 0.0,
            range: 0.0,
            lowest_z: 0.0,
        };

        if front.ceilingheight < back.ceilingheight {
            ww.top_z = front.ceilingheight;
        } else {
            ww.top_z = back.ceilingheight;
        }

        if front.floorheight > back.floorheight {
            ww.bottom_z = front.floorheight;
            ww.lowest_z = back.floorheight;
        } else {
            ww.bottom_z = back.floorheight;
            ww.lowest_z = front.floorheight;
        }
        ww.range = ww.top_z - ww.bottom_z;

        ww
    }
}

pub fn path_traverse(
    origin: Vec2,
    endpoint: Vec2,
    flags: i32,
    level: &mut Level,
    trav: impl FnMut(&mut Intercept) -> bool,
    bsp_trace: &mut BSPTrace,
) -> bool {
    let earlyout = flags & PT_EARLYOUT != 0;
    let mut intercepts: Vec<Intercept> = Vec::with_capacity(20);
    let trace = Trace::new(origin, endpoint - origin);

    level.valid_count = level.valid_count.wrapping_add(1);
    for n in bsp_trace.intercepted_subsectors() {
        if flags & PT_ADDLINES != 0 {
            let start = level.map_data.subsectors_mut()[*n as usize].start_seg as usize;
            let end = start + level.map_data.subsectors_mut()[*n as usize].seg_count as usize;

            for seg in &mut level.map_data.segments_mut()[start..end] {
                if seg.linedef.valid_count == level.valid_count {
                    continue;
                }
                seg.linedef.valid_count = level.valid_count;

                if !add_line_intercepts(trace, seg.linedef.clone(), &mut intercepts, earlyout) {
                    return false; // early out
                }
            }
        }

        if flags & PT_ADDTHINGS != 0
            && !level.map_data.subsectors_mut()[*n as usize]
                .sector
                .run_mut_func_on_thinglist(|thing| {
                    add_thing_intercept(trace, &mut intercepts, thing, level.valid_count)
                })
        {
            return false; // early out
        }
    }

    intercepts.sort();

    traverse_intercepts(&mut intercepts, 1.0, trav)
}

#[inline]
pub fn traverse_intercepts(
    intercepts: &mut [Intercept],
    max_frac: f32,
    mut trav: impl FnMut(&mut Intercept) -> bool,
) -> bool {
    if intercepts.is_empty() {
        return false;
    }
    let mut intercept: *mut Intercept = unsafe { intercepts.get_unchecked_mut(0) };
    let mut intercepts = Vec::from(intercepts);
    let mut count = intercepts.len();

    while count != 0 {
        count -= 1;
        let mut dist = f32::MAX;

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

            (*intercept).frac = f32::MAX;
        }
    }
    true
}

/// Check the line and add the intercept if valid
///
/// `line_to_line` is for "perfect" line-to-line collision (shot trace, use line
/// etc)
#[inline]
pub fn add_line_intercepts(
    trace: Trace,
    line: MapPtr<LineDef>,
    intercepts: &mut Vec<Intercept>,
    earlyout: bool,
) -> bool {
    let s1 = point_on_side(trace, line.v1);
    let s2 = point_on_side(trace, line.v2);

    if s1 == s2 {
        // line isn't crossed
        return true;
    }

    let dl = Trace::new(line.v1, line.v2 - line.v1);
    let frac = intercept_vector(trace, dl);
    // Skip if the trace doesn't intersect this line
    if frac.is_sign_negative() {
        return true;
    }

    if earlyout && frac < 1.0 && line.backsector.is_none() {
        return false;
    }

    if line.backsector.is_none() && frac < 0.0 {
        return false;
    }

    // TODO: early out
    intercepts.push(Intercept {
        frac,
        line: Some(line),
        thing: None,
    });
    true
}

// TODO: needs a proper line-line intersection test.
#[inline]
fn add_thing_intercept(
    trace: Trace,
    intercepts: &mut Vec<Intercept>,
    thing: &mut MapObject,
    valid_count: usize,
) -> bool {
    if thing.valid_count == valid_count {
        // Already checked it
        return true;
    }
    thing.valid_count = valid_count;

    // Diagonals are too unrealiable for first check so use
    // Use the seg check to limit the range
    if !circle_seg_collide(thing.xy, thing.radius, trace.xy, trace.xy + trace.dxy) {
        return true;
    }
    // Get vector clockwise-perpendicular to trace
    let r = thing.radius;
    let p = Vec2::new(trace.xy.y, -trace.xy.x).normalize() * r;
    let v1 = thing.xy + p;
    let v2 = thing.xy - p;

    let dl = Trace::new(v1, v2 - v1);
    let frac = intercept_vector(trace, dl);

    // println!("Passing through {:?}, from x{},y{}, to x{},y{}, r{} f{}",
    // thing.kind, trace.xy.x, trace.xy.y, thing.xy.x, thing.xy.y, thing.radius,
    // frac);

    // Skip if the trace doesn't intersect this line
    if frac.is_sign_negative() {
        return true;
    }

    intercepts.push(Intercept {
        frac,
        line: None,
        thing: Some(MapPtr::new(thing)),
    });
    true
}
