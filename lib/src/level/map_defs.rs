use std::ptr::{self, null_mut, NonNull};

use crate::{
    angle::Angle,
    play::{d_thinker::Thinker, map_object::MapObject},
    DPtr,
};
use glam::Vec2;

#[derive(Debug)]
pub enum SlopeType {
    Horizontal,
    Vertical,
    Positive,
    Negative,
}

/// The SECTORS record, at runtime.
/// Stores things/mobjs.
#[derive(Debug)]
pub struct Sector {
    /// An incremented "ID" of sorts.
    pub num: u32,
    pub floorheight: f32,
    pub ceilingheight: f32,
    /// Is a tag or index to patch
    pub floorpic: usize,
    /// Is a tag or index to patch
    pub ceilingpic: usize,
    pub lightlevel: i32,
    pub special: i16,
    pub tag: i16,

    /// 0 = untraversed, 1,2 = sndlines -1
    pub soundtraversed: i32,

    // thing that made a sound (or null)
    // TODO: mobj_t*	soundtarget;

    // mapblock bounding box for height changes
    pub blockbox: [i32; 4],

    // origin for any sounds played by the sector
    // TODO: degenmobj_t	soundorg;

    // if == validcount, already checked
    pub validcount: i32,

    // list of mobjs in sector
    pub thinglist: Option<NonNull<MapObject>>,

    // thinker_t for reversable actions
    pub specialdata: Option<*mut Thinker>,
    pub lines: Vec<DPtr<LineDef>>,
}

impl Sector {
    /// Returns false if `func` returns false
    pub fn run_func_on_thinglist(&mut self, mut func: impl FnMut(&mut MapObject) -> bool) -> bool {
        if let Some(thing) = self.thinglist.as_mut() {
            unsafe {
                let mut thing = thing.as_mut();

                loop {
                    if !func(thing) {
                        return false;
                    }

                    if let Some(mut next) = thing.s_next {
                        thing = next.as_mut()
                    } else {
                        break;
                    }
                }
            }
        }
        true
    }

    pub fn run_rfunc_on_thinglist(&self, mut func: impl FnMut(&MapObject) -> bool) -> bool {
        if let Some(thing) = self.thinglist.as_ref() {
            unsafe {
                let mut thing = thing.as_ref();

                loop {
                    if !func(thing) {
                        return false;
                    }

                    if let Some(mut next) = thing.s_next {
                        thing = next.as_mut()
                    } else {
                        break;
                    }
                }
            }
        }
        true
    }

    /// Add this thing tot he sectors thing list
    pub fn add_to_thinglist(&mut self, thing: &mut MapObject) {
        thing.s_prev = None;
        thing.s_next = self.thinglist; // could be null

        if let Some(other) = self.thinglist.as_mut() {
            unsafe {
                other.as_mut().s_prev = NonNull::new(thing);
            }
        }

        self.thinglist = NonNull::new(thing);
    }

    pub unsafe fn remove_from_thinglist(&mut self, thing: &mut MapObject) {
        if thing.s_next.is_none() && thing.s_prev.is_none() {
            self.thinglist = None;
        }

        if let Some(next) = thing.s_next.as_mut() {
            next.as_mut().s_prev = thing.s_prev; // could also be null
        }

        if let Some(prev) = thing.s_prev.as_mut() {
            prev.as_mut().s_next = thing.s_next;
        } else {
            (*thing.subsector).sector.thinglist = thing.s_next;
        }
    }
}

#[derive(Debug)]
pub struct SideDef {
    // add this to the calculated texture column
    pub textureoffset: f32,

    // add this to the calculated texture top
    pub rowoffset: f32,

    // TODO: link to textures by pointer?
    pub toptexture: usize,
    pub bottomtexture: usize,
    pub midtexture: usize,

    // Sector the SideDef is facing.
    pub sector: DPtr<Sector>,
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

        if v1.x() < v2.x() {
            bbox.left = v1.x();
            bbox.right = v2.x();
        } else {
            bbox.left = v2.x();
            bbox.right = v1.x();
        }

        if v1.y() < v2.y() {
            bbox.bottom = v1.y();
            bbox.top = v2.y();
        } else {
            bbox.bottom = v2.y();
            bbox.top = v1.y();
        }

        bbox
    }
}

#[derive(Debug)]
pub struct LineDef {
    // Vertices, from v1 to v2.
    pub v1: DPtr<Vec2>,
    pub v2: DPtr<Vec2>,

    // Precalculated v2 - v1 for side checking.
    pub delta: Vec2,

    // Animation related.
    pub flags: u32,
    pub special: i16,
    pub tag: i16,

    // Visual appearance: SideDefs.
    //  sidenum[1] will be -1 if one sided
    // Can leave this out as backsector.is_none() can check
    // pub sidenum: [i16; 2],

    // Neat. Another bounding box, for the extent
    //  of the LineDef.
    pub bbox: BBox,

    // To aid move clipping.
    pub slopetype: SlopeType,

    pub front_sidedef: DPtr<SideDef>,
    pub back_sidedef: Option<DPtr<SideDef>>,

    // Front and back sector.
    // Note: redundant? Can be retrieved from SideDefs.
    pub frontsector: DPtr<Sector>,
    pub backsector: Option<DPtr<Sector>>,

    // if == validcount, already checked
    pub valid_count: usize,
    // thinker_t for reversable actions
    // TODO: void*	specialdata: Option<DPtr<Thinker>>,
}

impl LineDef {
    pub fn point_on_side(&self, v: &Vec2) -> usize {
        // let r = (self.v2.x() - self.v1.x())*(v.y() - self.v1.y()) - (self.v2.y() - self.v1.y())*(v.x() - self.v1.x());
        // // dbg!(r);
        // if r.is_sign_positive() {
        //     return 1; // Back side
        // }
        // 0 // Front side

        let dx = v.x() - self.v1.x();
        let dy = v.y() - self.v1.y();

        if (dy * self.delta.x()) <= (self.delta.y() * dx) {
            // Front side
            return 0;
        }
        // Backside
        1
    }
}

#[derive(Debug)]
pub struct Segment {
    // Vertices, from v1 to v2.
    pub v1: DPtr<Vec2>,
    pub v2: DPtr<Vec2>,

    /// Offset distance along the linedef (from `start_vertex`) to the start
    /// of this `Segment`
    ///
    /// For diagonal `Segment` offset can be found with:
    /// `DISTANCE = SQR((x2 - x1)^2 + (y2 - y1)^2)`
    pub offset: f32,

    pub angle: Angle,

    pub sidedef: DPtr<SideDef>,
    /// The Linedef this segment travels along
    pub linedef: DPtr<LineDef>,

    pub frontsector: DPtr<Sector>,
    pub backsector: Option<DPtr<Sector>>,
}

impl Segment {
    /// True if the right side of the segment faces the point
    pub fn is_facing_point(&self, point: &Vec2) -> bool {
        let start = &self.v1;
        let end = &self.v2;

        let d = (end.y() - start.y()) * (start.x() - point.x())
            - (end.x() - start.x()) * (start.y() - point.y());
        if d <= f32::EPSILON {
            return true;
        }
        false
    }

    pub fn point_on_side(&self, v: &Vec2) -> usize {
        // let r = (self.v2.x() - self.v1.x())*(v.y() - self.v1.y()) - (self.v2.y() - self.v1.y())*(v.x() - self.v1.x());
        // // dbg!(r);
        // if r.is_sign_positive() {
        //     return 1; // Back side
        // }
        // 0 // Front side

        let dx = v.x() - self.v1.x();
        let dy = v.y() - self.v1.y();
        let this_delta = *self.v2 - *self.v1;

        if (dy * this_delta.x()) <= (this_delta.y() * dx) {
            // Front side
            return 0;
        }
        // Backside
        1
    }
}

#[derive(Debug)]
pub struct SubSector {
    pub sector: DPtr<Sector>,
    /// How many `Segment`s line this `SubSector`
    pub seg_count: i16,
    /// The `Segment` to start with
    pub start_seg: i16,
}

#[derive(Debug)]
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
    pub bounding_boxes: [[Vec2; 2]; 2],
    /// The node children. Doom uses a clever trick where if one node is selected
    /// then the other can also be checked with the same/minimal code by inverting
    /// the last bit
    pub child_index: [u16; 2],
    /// The parent of this node. Additional property to allow reversing up a BSP tree.
    pub parent: u16,
}

/// The `BLOCKMAP` is a pre-calculated structure that the game engine uses to simplify
/// collision-detection between moving things and walls.
///
/// Each "block" is 128 square
#[derive(Clone, Default)]
pub struct BlockMap {
    /// Leftmost X coord
    pub x_origin: f32,
    /// Bottommost Y coord
    pub y_origin: f32,
    /// Width
    pub width: i32,
    /// Height
    pub height: i32,
    /// Links to the MapObjects this block currently contains
    pub block_links: Vec<DPtr<MapObject>>,
    ///
    pub line_indexes: Vec<usize>,
    ///
    pub blockmap_offset: usize,
}

// impl BlockMap {
//     pub fn new(
//         x_origin: f32,
//         y_origin: f32,
//         width: i32,
//         height: i32,
//         block_links: Vec<DPtr<MapObject>>,
//         line_indexes: Vec<usize>,
//         blockmap_offset: usize,
//     ) -> BlockMap {
//         BlockMap {
//             x_origin,
//             y_origin,
//             width,
//             height,
//             block_links,
//             line_indexes,
//             blockmap_offset,
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use glam::Vec2;

    fn point_on_side(v1: Vec2, v2: Vec2, v: Vec2) -> usize {
        let r = (v2.x() - v1.x()) * (v.y() - v1.y()) - (v2.y() - v1.y()) * (v.x() - v1.x());
        // dbg!(r);
        if r.is_sign_positive() {
            return 1; // Back side
        }
        0 // Front side
    }

    #[test]
    fn line_side_problem() {
        // seg.v2.x() == 968.0 && seg.v2.y() == -2880.0 && seg.v1.x() == 832.0 && seg.v1.y() == -2944.0
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
