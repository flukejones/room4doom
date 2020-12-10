use crate::angle::Angle;
use crate::DPtr;
use glam::Vec2;

pub(crate) enum SlopeType {
    ST_HORIZONTAL,
    ST_VERTICAL,
    ST_POSITIVE,
    ST_NEGATIVE,
}

/// The SECTORS record, at runtime.
/// Stores things/mobjs.
pub(crate) struct Sector {
    pub floorheight:   f32,
    pub ceilingheight: f32,
    /// Is a tag or index to patch
    pub floorpic:      i16,
    /// Is a tag or index to patch
    pub ceilingpic:    i16,
    pub lightlevel:    i16,
    pub special:       i16,
    pub tag:           i16,

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
    // TODO: mobj_t*	thinglist;

    // thinker_t for reversable actions
    // TODO: void*	specialdata;
    pub linecount: i32,
    // TODO: index in to lines array, and iter by linecount
    //  struct line_s**	lines;	// [linecount] size
}

pub(crate) struct SideDef {
    // add this to the calculated texture column
    pub textureoffset: f32,

    // add this to the calculated texture top
    pub rowoffset: f32,

    // Texture indices.
    // We do not maintain names here.
    pub toptexture:    i16,
    pub bottomtexture: i16,
    pub midtexture:    i16,

    // Sector the SideDef is facing.
    pub sector: DPtr<Sector>,
}

#[derive(Default)]
pub(crate) struct BBox {
    pub top:    f32,
    pub bottom: f32,
    pub left:   f32,
    pub right:  f32,
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

pub(crate) struct LineDef {
    // Vertices, from v1 to v2.
    pub v1: DPtr<Vec2>,
    pub v2: DPtr<Vec2>,

    // Precalculated v2 - v1 for side checking.
    pub dx: f32,
    pub dy: f32,

    // Animation related.
    pub flags:   i16,
    pub special: i16,
    pub tag:     i16,

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
    pub back_sidedef:  Option<DPtr<SideDef>>,

    // Front and back sector.
    // Note: redundant? Can be retrieved from SideDefs.
    pub frontsector: DPtr<Sector>,
    pub backsector:  Option<DPtr<Sector>>,

    // if == validcount, already checked
    pub validcount: i32,
    // thinker_t for reversable actions
    // TODO: void*	specialdata: Option<DPtr<Thinker>>,
}

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
    pub backsector:  Option<DPtr<Sector>>,
}
