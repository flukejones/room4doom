use crate::angle::Angle;
use crate::thing::MapObject;
use crate::thinker::{Thinker, ThinkerData};
use crate::MapPtr;
use glam::Vec2;
use log::error;

#[derive(Debug)]
pub enum SlopeType {
    Horizontal,
    Vertical,
    Positive,
    Negative,
}

/// The SECTORS record, at runtime.
/// Stores things/mobjs.
#[derive(Default)]
pub struct Sector {
    /// An incremented "ID" of sorts.
    pub num: u32,
    pub floorheight: f32,
    pub ceilingheight: f32,
    /// Is a tag or index to patch
    pub floorpic: usize,
    /// Is a tag or index to patch
    pub ceilingpic: usize,
    pub lightlevel: usize,
    pub special: i16,
    pub tag: i16,

    /// 0 = untraversed, 1,2 = sndlines -1
    pub soundtraversed: i32,

    /// origin for any sounds played by the sector
    pub sound_origin: Vec2,

    // if == validcount, already checked
    pub validcount: usize,

    // list of mobjs in sector
    thinglist: Option<*mut Thinker>,

    // thinker_t for reversable actions
    pub specialdata: Option<*mut Thinker>,
    pub lines: Vec<MapPtr<LineDef>>,

    // thing that made a sound (or null)
    sound_target: Option<*mut Thinker>,
}

impl std::fmt::Debug for Sector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sector")
            .field("num", &self.num)
            .field("floorheight", &self.floorheight)
            .field("ceilingheight", &self.ceilingheight)
            .field("floorpic", &self.floorpic)
            .field("ceilingpic", &self.ceilingpic)
            .finish_non_exhaustive()
    }
}

impl Sector {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        num: u32,
        floorheight: f32,
        ceilingheight: f32,
        floorpic: usize,
        ceilingpic: usize,
        lightlevel: usize,
        special: i16,
        tag: i16,
    ) -> Self {
        Self {
            num,
            floorheight,
            ceilingheight,
            floorpic,
            ceilingpic,
            lightlevel,
            special,
            tag,
            ..Self::default()
        }
    }

    /// Returns false if `func` returns false
    pub fn run_mut_func_on_thinglist(
        &mut self,
        mut func: impl FnMut(&mut MapObject) -> bool,
    ) -> bool {
        if let Some(thing) = self.thinglist {
            #[cfg(feature = "null_check")]
            if thing.is_null() {
                std::panic!("thinglist is null when it shouldn't be");
            }
            unsafe {
                if (*thing).should_remove() {
                    return true;
                }
                let mut thing = (*thing).mobj_mut();

                loop {
                    // Thing might remove itself so grab a copy of s_next here
                    let next = thing.s_next;
                    if !func(thing) {
                        return false;
                    }

                    if let Some(next) = next {
                        #[cfg(feature = "null_check")]
                        if next.is_null() {
                            std::panic!("thinglist thing.s_next is null when it shouldn't be");
                        }
                        // Skip items that may have been marked for removal
                        if (*next).should_remove() {
                            continue;
                        }
                        thing = (*next).mobj_mut()
                    } else {
                        break;
                    }
                }
            }
        }
        true
    }

    pub fn run_func_on_thinglist(&self, mut func: impl FnMut(&MapObject) -> bool) -> bool {
        if let Some(thing) = self.thinglist {
            #[cfg(feature = "null_check")]
            if thing.is_null() {
                std::panic!("thinglist is null when it shouldn't be");
            }
            unsafe {
                if (*thing).should_remove() {
                    return true;
                }
                let mut thing = (*thing).mobj();

                loop {
                    // Thing might remove itself so grab a copy of s_next here
                    let next = thing.s_next;
                    if !func(thing) {
                        return false;
                    }

                    if let Some(next) = next {
                        #[cfg(feature = "null_check")]
                        if next.is_null() {
                            std::panic!("thinglist thing.s_next is null when it shouldn't be");
                        }
                        // Skip items that may have been marked for removal
                        if (*next).should_remove() {
                            continue;
                        }
                        thing = (*next).mobj()
                    } else {
                        break;
                    }
                }
            }
        }
        true
    }

    /// Add this thing to the sectors thing list
    ///
    /// # Safety
    /// The `Thinker` pointer *must* be valid, and the `Thinker` must not be
    /// `Free` or `Remove`
    pub unsafe fn add_to_thinglist(&mut self, thing: *mut Thinker) {
        if matches!((*thing).data(), ThinkerData::Free | ThinkerData::Remove) {
            error!("add_to_thinglist() tried to add a Thinker that was Free or Remove");
            return;
        }
        (*thing).mobj_mut().s_prev = None;
        (*thing).mobj_mut().s_next = self.thinglist; // could be null

        if let Some(other) = self.thinglist {
            (*other).mobj_mut().s_prev = Some(thing);
        }

        self.thinglist = Some(thing);
    }

    /// Add this thing to this sectors thinglist
    ///
    /// # Safety
    /// Must be called if a thing is ever removed
    pub unsafe fn remove_from_thinglist(&mut self, thing: &mut Thinker) {
        if thing.mobj().s_next.is_none() && thing.mobj().s_prev.is_none() {
            self.thinglist = None;
        }

        if let Some(next) = thing.mobj().s_next {
            (*next).mobj_mut().s_prev = (*thing).mobj_mut().s_prev;
            // could also be null
        }

        if let Some(prev) = thing.mobj().s_prev {
            (*prev).mobj_mut().s_next = thing.mobj_mut().s_next;
        } else {
            let mut ss = thing.mobj().subsector.clone();
            ss.sector.thinglist = thing.mobj().s_next;
        }
    }

    pub fn sound_target(&self) -> Option<&mut MapObject> {
        self.sound_target.map(|m| unsafe { (*m).mobj_mut() })
    }

    pub fn sound_target_raw(&mut self) -> Option<*mut Thinker> {
        self.sound_target
    }

    pub fn set_sound_target(&mut self, target: *mut Thinker) {
        self.sound_target = Some(target);
    }
}

#[derive(Debug)]
pub struct SideDef {
    // add this to the calculated texture column
    pub textureoffset: f32,

    // add this to the calculated texture top
    pub rowoffset: f32,

    // TODO: link to textures by pointer?
    pub toptexture: Option<usize>,
    pub bottomtexture: Option<usize>,
    pub midtexture: Option<usize>,

    // Sector the SideDef is facing.
    pub sector: MapPtr<Sector>,
}

#[derive(Debug, Default)]
pub struct BBox {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl BBox {
    pub fn new(v1: Vec2, v2: Vec2) -> Self {
        let mut bbox = BBox::default();

        if v1.x < v2.x {
            bbox.left = v1.x;
            bbox.right = v2.x;
        } else {
            bbox.left = v2.x;
            bbox.right = v1.x;
        }

        if v1.y < v2.y {
            bbox.bottom = v1.y;
            bbox.top = v2.y;
        } else {
            bbox.bottom = v2.y;
            bbox.top = v1.y;
        }

        bbox
    }
}

pub struct LineDef {
    // Vertices, from v1 to v2.
    pub v1: Vec2,
    pub v2: Vec2,
    // Precalculated v2 - v1 for side checking.
    pub delta: Vec2,
    // Animation related.
    pub flags: u32,
    pub special: i16,
    pub tag: i16,

    // Neat. Another bounding box, for the extent
    //  of the LineDef.
    pub bbox: BBox,
    // To aid move clipping.
    pub slopetype: SlopeType,

    /// Convenience
    pub sides: [u16; 2],
    // Visual appearance: SideDefs.
    //  sidenum[1] will be -1 if one sided
    /// Helper to prevent having to lookup the sidedef. Used for interaction stuff or setting the textures to draw but not used during drawing.
    pub front_sidedef: MapPtr<SideDef>,
    /// Helper to prevent having to lookup the sidedef. Used for interaction stuff or setting the textures to draw but not used during drawing.
    pub back_sidedef: Option<MapPtr<SideDef>>,

    // Front and back sector.
    pub frontsector: MapPtr<Sector>,
    pub backsector: Option<MapPtr<Sector>>,

    // if == validcount, already checked
    pub valid_count: usize,
    // thinker_t for reversable actions
    // TODO: void*	specialdata: Option<MapPtr<Thinker>>,
}

impl std::fmt::Debug for LineDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Linedef")
            .field("v1", &self.v1)
            .field("v2", &self.v2)
            .field("flags", &self.flags)
            .field("tag", &self.tag)
            .field("bbox", &self.bbox)
            .field("slopetype", &self.slopetype)
            .field("front_sidedef", &self.front_sidedef)
            .field("back_sidedef", &self.back_sidedef)
            // .field("valid_count", &self.valid_count)
            .finish_non_exhaustive()
    }
}

impl LineDef {
    /// True if the right side of the segment faces the point
    pub fn is_facing_point(&self, point: &Vec2) -> bool {
        let start = &self.v1;
        let end = &self.v2;

        let d = (end.y - start.y) * (start.x - point.x) - (end.x - start.x) * (start.y - point.y);
        if d >= 0.0 {
            return false;
        }
        true
    }

    pub fn point_on_side(&self, v: Vec2) -> usize {
        // let r = (self.v2.x - self.v1.x)*(v.y - self.v1.y) - (self.v2.y -
        // self.v1.y)*(v.x - self.v1.x); // dbg!(r);
        // if r.is_sign_positive() {
        //     return 1; // Back side
        // }
        // 0 // Front side

        let dx = v.x - self.v1.x;
        let dy = v.y - self.v1.y;

        if (dy * self.delta.x) <= (self.delta.y * dx) {
            // Front side
            return 0;
        }
        // Backside
        1
    }
}

#[derive(Debug, Clone)]
pub struct Segment {
    // Vertices, from v1 to v2.
    pub v1: Vec2,
    pub v2: Vec2,

    /// Offset distance along the linedef (from `start_vertex`) to the start
    /// of this `Segment`
    pub offset: f32,
    pub angle: Angle,

    pub sidedef: MapPtr<SideDef>,
    /// The Linedef this segment travels along. During drawing it is used for finding flags.
    pub linedef: MapPtr<LineDef>,

    pub frontsector: MapPtr<Sector>,
    pub backsector: Option<MapPtr<Sector>>,
}

impl Segment {
    pub fn test_panic(&self) -> bool {
        // vertex:
        // 12 top-left (256.0, -1392.0)
        // 4176 top-right (272.0, -1392.0)
        // 4143 bottom-right (272.0, -1408.0)
        if self.v2 == Vec2::new(256., -1392.) && self.v1 == Vec2::new(272., -1392.) {
            dbg!(self.sidedef.bottomtexture);
            dbg!(&self.linedef.front_sidedef);
            dbg!(&self.linedef.back_sidedef);
            dbg!(&self.frontsector);
            dbg!(&self.backsector);
            dbg!(self.sidedef != self.linedef.front_sidedef);
            return true;
        }
        false
    }

    /// Helper to recalcuate the offset of a seg along the linedef line it is
    /// derived from. Required for ZDBSP style nodes.
    pub fn recalc_offset(v1: Vec2, v2: Vec2) -> f32 {
        let a = v1.x - v2.x;
        let b = v1.y - v2.y;
        (a * a + b * b).sqrt()
    }

    /// True if the right side of the segment faces the point
    pub fn is_facing_point(&self, point: &Vec2) -> bool {
        let start = &self.v1;
        let end = &self.v2;

        let d = (end.y - start.y) * (start.x - point.x) - (end.x - start.x) * (start.y - point.y);
        if d <= 0.1 {
            return true;
        }
        false
    }

    pub fn point_on_side(&self, v: &Vec2) -> usize {
        // let r = (self.v2.x - self.v1.x)*(v.y - self.v1.y) - (self.v2.y -
        // self.v1.y)*(v.x - self.v1.x); // dbg!(r);
        // if r.is_sign_positive() {
        //     return 1; // Back side
        // }
        // 0 // Front side

        let dx = v.x - self.v1.x;
        let dy = v.y - self.v1.y;
        let this_delta = self.v2 - self.v1;

        if (dy * this_delta.x) <= (this_delta.y * dx) {
            // Front side
            return 0;
        }
        // Backside
        1
    }
}

#[derive(Debug)]
pub struct SubSector {
    pub sector: MapPtr<Sector>,
    /// How many `Segment`s line this `SubSector`
    pub seg_count: u32,
    /// The `Segment` to start with
    pub start_seg: u32,
}

#[derive(Debug, PartialEq)]
pub struct Node {
    /// Where the line used for splitting the level starts
    pub xy: Vec2,
    /// Where the line used for splitting the level ends
    pub delta: Vec2,
    /// Coordinates of the bounding boxes:
    /// - [0][0] == right box, top-left
    /// - [0][1] == right box, bottom-right
    /// - [1][0] == left box, top-left
    /// - [1][1] == left box, bottom-right
    pub bboxes: [[Vec2; 2]; 2],
    /// The node children. Doom uses a clever trick where if one node is
    /// selected then the other can also be checked with the same/minimal
    /// code by inverting the last bit.
    /// The final 'leaf' is bitmasked to find the index to subsector array
    pub children: [u32; 2],
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    fn point_on_side(v1: Vec2, v2: Vec2, v: Vec2) -> usize {
        let r = (v2.x - v1.x) * (v.y - v1.y) - (v2.y - v1.y) * (v.x - v1.x);
        // dbg!(r);
        if r.is_sign_positive() {
            return 1; // Back side
        }
        0 // Front side
    }

    #[test]
    fn line_side_problem() {
        // seg.v2.x == 968.0 && seg.v2.y == -2880.0 && seg.v1.x == 832.0 && seg.v1.y ==
        // -2944.0
        let v1 = Vec2::new(832.0, -2944.0);
        let v2 = Vec2::new(968.0, -2880.0);

        let v = Vec2::new(0.0, 0.0);
        let r = point_on_side(v1, v2, v);
        assert_eq!(r, 1);

        let v = Vec2::new(976.0, -2912.0);
        let r = point_on_side(v1, v2, v);
        assert_eq!(r, 0);
    }
}
