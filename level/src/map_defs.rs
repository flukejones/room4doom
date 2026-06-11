use crate::MapPtr;
use crate::flags::LineDefFlags;
use glam::Vec2;
use math::{Angle, FixedT, p_aprox_distance, r_point_on_side_raw};
use rbsp::SlopePlane;

/// Sector floor/ceiling height type: fixed-point for deterministic gameplay.
pub type SectorHeight = FixedT;

/// Map vertex with both f32 position and original fixed-point coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub pos: Vec2,
    pub x_fp: FixedT,
    pub y_fp: FixedT,
}

impl Vertex {
    pub fn new(x: f32, y: f32, x_fp: FixedT, y_fp: FixedT) -> Self {
        Self {
            pos: Vec2::new(x, y),
            x_fp,
            y_fp,
        }
    }
}

impl PartialEq for Vertex {
    fn eq(&self, other: &Self) -> bool {
        self.pos == other.pos
    }
}

impl std::ops::Deref for Vertex {
    type Target = Vec2;
    fn deref(&self) -> &Vec2 {
        &self.pos
    }
}

impl std::ops::DerefMut for Vertex {
    fn deref_mut(&mut self) -> &mut Vec2 {
        &mut self.pos
    }
}

/// The graph slope kind when looking at the map from top down.
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
    pub num: i32,
    pub floorheight: SectorHeight,
    pub ceilingheight: SectorHeight,
    /// Is a tag or index to patch
    pub floorpic: usize,
    /// Is a tag or index to patch
    pub ceilingpic: usize,
    pub lightlevel: usize,
    pub special: u32,
    pub tag: i16,
    /// Saved special for reference
    pub default_special: i16,
    /// Saved tag for reference
    pub default_tag: i16,

    /// 0 = untraversed, 1,2 = sndlines -1
    pub soundtraversed: i32,

    /// origin for any sounds played by the sector
    pub sound_origin: Vec2,

    // if == validcount, already checked
    pub validcount: usize,

    /// list of mobjs in sector (opaque pointer to Thinker in gameplay)
    pub thinglist: Option<*mut ()>,

    /// thinker_t for reversable actions (opaque pointer to Thinker in gameplay)
    pub specialdata: Option<*mut ()>,
    pub lines: Vec<MapPtr<LineDef>>,

    /// Previous tic values for rendering interpolation.
    pub prev_floorheight: SectorHeight,
    pub prev_ceilingheight: SectorHeight,
    pub prev_lightlevel: usize,
    /// True post-tic values saved during interpolation, restored after render.
    pub interp_floorheight: SectorHeight,
    pub interp_ceilingheight: SectorHeight,
    pub interp_lightlevel: usize,

    /// Blockmap bounding box in block coordinates [top, bottom, right, left]
    pub blockbox: [i32; 4],

    /// thing that made a sound (or null) (opaque pointer to Thinker in
    /// gameplay)
    pub sound_target: Option<*mut ()>,

    /// Sloped floor plane (UDMF); `None` = flat at `floorheight`.
    pub floor_plane: Option<SlopePlane>,
    /// Sloped ceiling plane (UDMF); `None` = flat at `ceilingheight`.
    pub ceil_plane: Option<SlopePlane>,
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
        floorheight: SectorHeight,
        ceilingheight: SectorHeight,
        floorpic: usize,
        ceilingpic: usize,
        lightlevel: usize,
        special: i16,
        tag: i16,
    ) -> Self {
        Self {
            num: num as i32,
            floorheight,
            ceilingheight,
            floorpic,
            ceilingpic,
            lightlevel,
            special: special as u32,
            tag,
            default_special: special,
            default_tag: tag,
            prev_floorheight: floorheight,
            prev_ceilingheight: ceilingheight,
            prev_lightlevel: lightlevel,
            ..Self::default()
        }
    }

    pub fn set_sound_target(&mut self, target: *mut ()) {
        self.sound_target = Some(target);
    }

    /// Floor z at `(x, y)`: the slope plane when sloped, else `floorheight`.
    /// `x`/`y` are fixed-point map coords; plane eval crosses to f32 and back.
    pub fn floor_z(&self, x: FixedT, y: FixedT) -> FixedT {
        match self.floor_plane {
            Some(p) => FixedT::from_f32(p.z_at(x.to_f32(), y.to_f32())),
            None => self.floorheight,
        }
    }

    /// Ceiling z at `(x, y)`: the slope plane when sloped, else `ceilingheight`.
    pub fn ceil_z(&self, x: FixedT, y: FixedT) -> FixedT {
        match self.ceil_plane {
            Some(p) => FixedT::from_f32(p.z_at(x.to_f32(), y.to_f32())),
            None => self.ceilingheight,
        }
    }
}

#[derive(Debug)]
pub struct SideDef {
    // add this to the calculated texture column
    pub textureoffset: FixedT,

    // add this to the calculated texture top
    pub rowoffset: FixedT,

    // TODO: link to textures by pointer?
    pub toptexture: Option<usize>,
    pub bottomtexture: Option<usize>,
    pub midtexture: Option<usize>,

    // Sector the SideDef is facing.
    pub sector: MapPtr<Sector>,
}

#[derive(Debug, Default, Clone)]
pub struct BBox {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl BBox {
    pub fn new(v1: Vec2, v2: Vec2) -> Self {
        let mut bbox = Self::default();

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
    /// Index position of self. Used as ID for when checking through ref chain.
    pub num: usize,
    // Vertices, from v1 to v2.
    pub v1: MapPtr<Vertex>,
    pub v2: MapPtr<Vertex>,
    // Precalculated v2 - v1 for side checking.
    pub delta: Vec2,
    /// Delta in raw 16.16 fixed-point, computed from vertex x_fp/y_fp.
    pub delta_fp: [i32; 2],
    // Animation related.
    pub flags: LineDefFlags,
    pub special: u32,
    pub tag: i16,

    /// Saved special for reference
    pub default_special: i16,
    /// Saved tag for reference
    pub default_tag: i16,

    // Neat. Another bounding box, for the extent
    //  of the LineDef.
    pub bbox: BBox,
    /// Bounding box in 16.16 fixed-point integers, indexed as
    /// `[BOXTOP, BOXBOTTOM, BOXLEFT, BOXRIGHT]`. Used for exact integer
    /// overlap checks matching OG Doom precision.
    pub bbox_int: [i32; 4],
    // To aid move clipping.
    pub slopetype: SlopeType,

    /// Convenience
    pub sides: [u16; 2],
    // Visual appearance: SideDefs.
    //  sidenum[1] will be -1 if one sided
    /// Helper to prevent having to lookup the sidedef. Used for interaction
    /// stuff or setting the textures to draw but not used during drawing.
    pub front_sidedef: MapPtr<SideDef>,
    /// Helper to prevent having to lookup the sidedef. Used for interaction
    /// stuff or setting the textures to draw but not used during drawing.
    pub back_sidedef: Option<MapPtr<SideDef>>,

    // Front and back sector.
    pub frontsector: MapPtr<Sector>,
    pub backsector: Option<MapPtr<Sector>>,

    // if == validcount, already checked
    pub valid_count: usize,
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
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct Segment {
    // Vertices, from v1 to v2.
    pub v1: MapPtr<Vertex>,
    pub v2: MapPtr<Vertex>,

    /// Offset distance along the linedef (from `start_vertex`) to the start
    /// of this `Segment`
    pub offset: FixedT,
    pub angle: Angle,

    pub sidedef: MapPtr<SideDef>,
    /// The Linedef this segment travels along. During drawing it is used for
    /// finding flags.
    pub linedef: MapPtr<LineDef>,

    pub frontsector: MapPtr<Sector>,
    pub backsector: Option<MapPtr<Sector>>,

    /// Index of the subsector this segment encloses.
    pub subsector: usize,
    /// Subsector indices across this segment (other side, overlapping span).
    /// Empty for one-sided segments; more than one where sides split unevenly.
    pub back_subsectors: Vec<usize>,
}

impl Segment {
    /// Helper to recalcuate the offset of a seg along the linedef line it is
    /// derived from. Required for ZDBSP style nodes.
    pub fn recalc_offset(v1: &Vertex, v2: &Vertex) -> FixedT {
        p_aprox_distance(v1.x_fp - v2.x_fp, v1.y_fp - v2.y_fp)
    }

    /// OG Doom `R_PointOnSegSide` — overflow-safe 16.16 side test against this
    /// seg (origin `v1`, delta `v1`→`v2`). Returns 0 for the front (right)
    /// side, 1 for the back.
    #[inline]
    pub fn point_on_side(&self, x: FixedT, y: FixedT) -> usize {
        let lx = self.v1.x_fp.to_fixed_raw();
        let ly = self.v1.y_fp.to_fixed_raw();
        r_point_on_side_raw(
            x.to_fixed_raw(),
            y.to_fixed_raw(),
            lx,
            ly,
            self.v2.x_fp.to_fixed_raw().wrapping_sub(lx),
            self.v2.y_fp.to_fixed_raw().wrapping_sub(ly),
        )
    }
}

#[derive(Debug, PartialEq)]
pub struct SubSector {
    pub sector: MapPtr<Sector>,
    /// How many `Segment`s line this `SubSector`
    pub seg_count: u32,
    /// The `Segment` to start with
    pub start_seg: u32,
}

/// OG Doom blockmap: 128×128 unit grid for spatial line/thing queries.
/// Lines stored in CSR (compressed sparse row) format.
#[derive(Default)]
pub struct Blockmap {
    /// X origin in 16.16 fixed-point
    pub x_origin: i32,
    /// Y origin in 16.16 fixed-point
    pub y_origin: i32,
    pub columns: i32,
    pub rows: i32,
    /// CSR offsets: `block_offsets[y * columns + x]` .. `block_offsets[y *
    /// columns + x + 1]` indexes into `block_lines`. Length = columns *
    /// rows + 1.
    pub block_offsets: Vec<usize>,
    /// Flat array of linedef pointers grouped by block
    pub block_lines: Vec<MapPtr<LineDef>>,
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    fn point_on_side(v1: Vec2, v2: Vec2, v: Vec2) -> usize {
        let r = (v2.x - v1.x) * (v.y - v1.y) - (v2.y - v1.y) * (v.x - v1.x);
        if r.is_sign_positive() {
            return 1; // Back side
        }
        0 // Front side
    }

    #[test]
    fn line_side_problem() {
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
