// TODO: Structures, in WAD order
//  - [X] Thing
//  - [X] LineDef
//  - [X] SideDef
//  - [X] Vertex
//  - [X] Segment   (SEGS)
//  - [X] SubSector (SSECTORS)
//  - [ ] Node
//  - [X] Sector
//  - [ ] Reject
//  - [ ] Blockmap

use crate::DPtr;
use std::ptr::NonNull;
use std::str;

// TODO: A `Thing` type will need to be mapped against an enum
#[derive(Debug)]
pub struct Thing {
    pub pos_x: i16,
    pub pos_y: i16,
    pub angle: u16,
    pub typ: u16,
    pub flags: u16,
}

impl Thing {
    pub fn new(pos_x: i16, pos_y: i16, angle: u16, typ: u16, flags: u16) -> Thing {
        Thing {
            pos_x,
            pos_y,
            angle,
            typ,
            flags,
        }
    }
}

/// The flags control some attributes of the line
pub enum LineDefFlags {
    /// Players and monsters cannot cross this line. Note that
    /// if there is no sector on the other side, they can't go through the line
    /// anyway, regardless of the flags
    Blocking = 1,
    /// Monsters cannot cross this line
    BlockMonsters = 1 << 1,
    /// The linedef's two sidedefs can have "-" as a texture,
    /// which in this case means "transparent". If this flag is not set, the
    /// sidedefs can't be transparent. A side effect of this flag is that if
    /// it is set, then gunfire (pistol, shotgun, chaingun) can go through it
    TwoSided = 1 << 2,
    /// The upper texture is pasted onto the wall from
    /// the top down instead of from the bottom up like usual.
    /// The effect is if a wall moves down, it looks like the
    /// texture is stationary and is appended to as the wall moves
    UnpegTop = 1 << 3,
    /// Lower and middle textures are drawn from the
    /// bottom up, instead of from the top down like usual
    /// The effect is if a wall moves up, it looks like the
    /// texture is stationary and is appended to as the wall moves
    UnpegBottom = 1 << 4,
    /// On the automap, this line appears in red like a normal
    /// solid wall that has nothing on the other side. This is useful in
    /// protecting secret doors and such. Note that if the sector on the other
    /// side of this "secret" line has its floor height HIGHER than the sector
    /// on the facing side of the secret line, then the map will show the lines
    /// beyond and thus give up the secret
    Secret = 1 << 5,
    /// For purposes of monsters hearing sounds and thus
    /// becoming alerted. Every time a player fires a weapon, the "sound" of
    /// it travels from sector to sector, alerting all non-deaf monsters in
    /// each new sector. This flag blocks sound traveling out of this sector
    /// through this line to adjacent sector
    BlockSound = 1 << 6,
    /// Not on AutoMap
    DontDraw = 1 << 7,
    /// Already on AutoMap
    Draw = 1 << 8,
}

#[derive(Debug, Default)]
pub struct Vertex {
    pub x: i16,
    pub y: i16,
}

impl Vertex {
    pub fn new(x: i16, y: i16) -> Vertex {
        Vertex { x, y }
    }
}

/// Each linedef represents a line from one of the VERTEXES to another,
/// and each linedef's record is 14 bytes, and is made up of 7 16-bit
/// fields
#[derive(Debug)]
pub struct LineDef {
    /// The line starts from this point
    pub start_vertex: DPtr<Vertex>,
    /// The line ends at this point
    pub end_vertex: DPtr<Vertex>,
    /// The line attributes, see `LineDefFlags`
    pub flags: u16,
    pub line_type: u16,
    /// This is a number which ties this line's effect type
    /// to all SECTORS that have the same tag number (in their last
    /// field)
    pub sector_tag: u16,
    /// Index number of the front `SideDef` for this line
    pub front_sidedef: DPtr<SideDef>, //0xFFFF means there is no sidedef
    /// Index number of the back `SideDef` for this line
    pub back_sidedef: Option<DPtr<SideDef>>, //0xFFFF means there is no sidedef
}

impl LineDef {
    pub fn new(
        start_vertex: DPtr<Vertex>,
        end_vertex: DPtr<Vertex>,
        flags: u16,
        line_type: u16,
        sector_tag: u16,
        front_sidedef: DPtr<SideDef>,
        back_sidedef: Option<DPtr<SideDef>>,
    ) -> LineDef {
        LineDef {
            start_vertex,
            end_vertex,
            flags,
            line_type,
            sector_tag,
            front_sidedef,
            back_sidedef,
        }
    }
}

/// The Segments (SEGS) are in a sequential order determined by the `SubSector`
/// (SSECTOR), which are part of the NODES recursive tree
///
/// Each `Segment` record is 12 bytes
#[derive(Debug)]
pub struct Segment {
    /// The line starts from this point
    pub start_vertex: DPtr<Vertex>,
    /// The line ends at this point
    pub end_vertex: DPtr<Vertex>,
    /// Binary Angle Measurement
    pub angle: u16,
    /// The Linedef this segment travels along
    pub linedef: DPtr<LineDef>,
    pub direction: u16,
    /// Offset distance along the linedef (from `start_vertex`) to the start
    /// of this `Segment`
    ///
    /// For diagonal `Segment` offset can be found with:
    /// `DISTANCE = SQR((x2 - x1)^2 + (y2 - y1)^2)`
    pub offset: u16,
}

impl Segment {
    pub fn new(
        start_vertex: DPtr<Vertex>,
        end_vertex: DPtr<Vertex>,
        angle: u16,
        linedef: DPtr<LineDef>,
        direction: u16,
        offset: u16,
    ) -> Segment {
        Segment {
            start_vertex,
            end_vertex,
            angle,
            linedef,
            direction,
            offset,
        }
    }
}

/// A `SubSector` divides up all the SECTORS into convex polygons. They are then
/// referenced through the NODES resources. There will be (number of nodes) + 1.
///
/// Each `SubSector` record is 4 bytes
#[derive(Debug)]
pub struct SubSector {
    /// How many `Segment`s line this `SubSector`
    pub seg_count: u16,
    /// The `Segment` to start with
    pub start_seg: u16,
}

impl SubSector {
    pub fn new(seg_count: u16, start_seg: u16) -> SubSector {
        SubSector {
            seg_count,
            start_seg,
        }
    }
}

/// A `Sector` is a horizontal (east-west and north-south) area of the map
/// where a floor height and ceiling height is defined.
/// Any change in floor or ceiling height or texture requires a
/// new sector (and therefore separating linedefs and sidedefs).
///
/// Each `Sector` record is 26 bytes
#[derive(Debug)]
pub struct Sector {
    pub floor_height: i16,
    pub ceil_height: i16,
    /// Floor texture name
    pub floor_tex: String,
    /// Ceiling texture name
    pub ceil_tex: String,
    /// Light level from 0-255. There are actually only 32 brightnesses
    /// possible so blocks of 8 are the same bright
    pub light_level: u16,
    /// This determines some area-effects called special sectors
    pub typ: u16,
    /// a "tag" number corresponding to LINEDEF(s) with the same tag
    /// number. When that linedef is activated, something will usually
    /// happen to this sector - its floor will rise, the lights will
    /// go out, etc
    pub tag: u16,
}

impl Sector {
    pub fn new(
        floor_height: i16,
        ceil_height: i16,
        floor_tex: &[u8],
        ceil_tex: &[u8],
        light_level: u16,
        typ: u16,
        tag: u16,
    ) -> Sector {
        if floor_tex.len() != 8 {
            panic!(
                "sector floor_tex name incorrect length, expected 8, got {}",
                floor_tex.len()
            )
        }
        if ceil_tex.len() != 8 {
            panic!(
                "sector ceil_tex name incorrect length, expected 8, got {}",
                ceil_tex.len()
            )
        }
        Sector {
            floor_height,
            ceil_height,
            floor_tex: str::from_utf8(floor_tex)
                .expect("Invalid floor tex name")
                .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                .to_owned(),
            ceil_tex: str::from_utf8(ceil_tex)
                .expect("Invalid ceiling tex name")
                .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                .to_owned(),
            light_level,
            typ,
            tag,
        }
    }
}

/// A sidedef is a definition of what wall texture(s) to draw along a
/// `LineDef`, and a group of sidedefs outline the space of a `Sector`
///
/// Each `SideDef` record is 30 bytes
#[derive(Debug)]
pub struct SideDef {
    pub x_offset: i16,
    pub y_offset: i16,
    /// Name of upper texture used for example in the upper of a window
    pub upper_tex: String,
    /// Name of lower texture used for example in the front of a step
    pub lower_tex: String,
    /// The regular part of a wall
    pub middle_tex: String,
    /// Sector that this sidedef faces or helps to surround
    pub sector: DPtr<Sector>,
}

impl SideDef {
    pub fn new(
        x_offset: i16,
        y_offset: i16,
        upper_tex: &[u8],
        lower_tex: &[u8],
        middle_tex: &[u8],
        sector: DPtr<Sector>,
    ) -> SideDef {
        if upper_tex.len() != 8 {
            panic!(
                "sidedef upper_tex name incorrect length, expected 8, got {}",
                upper_tex.len()
            )
        }
        if lower_tex.len() != 8 {
            panic!(
                "sidedef lower_tex name incorrect length, expected 8, got {}",
                lower_tex.len()
            )
        }
        if middle_tex.len() != 8 {
            panic!(
                "sidedef middle_tex name incorrect length, expected 8, got {}",
                middle_tex.len()
            )
        }
        SideDef {
            x_offset,
            y_offset,
            upper_tex: str::from_utf8(upper_tex)
                .expect("Invalid upper_tex name")
                .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                .to_owned(),
            lower_tex: str::from_utf8(lower_tex)
                .expect("Invalid lower_tex name")
                .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                .to_owned(),
            middle_tex: str::from_utf8(middle_tex)
                .expect("Invalid middle_tex name")
                .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                .to_owned(),
            sector,
        }
    }
}
