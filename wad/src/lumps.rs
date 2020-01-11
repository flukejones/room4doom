// TODO: Structures, in WAD order
//  - [X] Thing
//  - [X] LineDef
//  - [X] SideDef
//  - [X] Vertex
//  - [X] Segment   (SEGS)
//  - [X] SubSector (SSECTORS)
//  - [X] Node
//  - [X] Sector
//  - [ ] Reject
//  - [ ] Blockmap

pub use crate::nodes::{Node, IS_SSECTOR_MASK};
use crate::DPtr;
use crate::Vertex;
use std::str;

/// A `Thing` describes only the position, type, and angle + spawn flags
///
/// The data in the WAD lump is structured as follows:
///
/// | Field Size | Data Type | Content    |
/// |------------|-----------|------------|
/// |  0x00-0x01 |    i16    | X Position |
/// |  0x02-0x03 |    i16    | Y Position |
/// |  0x04-0x05 |    u16    | Angle      |
/// |  0x06-0x07 |    u16    | Type       |
/// |  0x08-0x09 |    u16    | Flags      |
///
/// Each `Thing` record is 10 bytes
// TODO: A `Thing` type will need to be mapped against an enum
#[derive(Debug)]
pub struct Thing {
    pub pos: Vertex,
    pub angle: f32,
    pub kind: u16,
    pub flags: u16,
}

impl Thing {
    pub fn new(pos: Vertex, angle: f32, kind: u16, flags: u16) -> Thing {
        Thing {
            pos,
            angle,
            kind,
            flags,
        }
    }
}

/// A `Vertex` is the basic struct used for any type of coordinate
/// in the game
///
/// The data in the WAD lump is structured as follows:
///
/// | Field Size | Data Type | Content      |
/// |------------|-----------|--------------|
/// |  0x00-0x01 |    i16    | X Coordinate |
/// |  0x02-0x03 |    i16    | Y Coordinate |
// TODO: Use the Vec2d module
#[derive(Debug, Default, Clone)]
struct WVertex {
    x: f32,
    y: f32,
}

/// Each linedef represents a line from one of the VERTEXES to another.
///
/// The data in the WAD lump is structured as follows:
///
///| Field Size | Data Type      | Content                                   |
///|------------|----------------|-------------------------------------------|
///|  0x00-0x01 | Unsigned short | Start vertex                              |
///|  0x02-0x03 | Unsigned short | End vertex                                |
///|  0x04-0x05 | Unsigned short | Flags (details below)                     |
///|  0x06-0x07 | Unsigned short | Line type / Action                        |
///|  0x08-0x09 | Unsigned short | Sector tag                                |
///|  0x10-0x11 | Unsigned short | Front sidedef ( 0xFFFF side not present ) |
///|  0x12-0x13 | Unsigned short | Back sidedef  ( 0xFFFF side not present ) |
///
/// Each linedef's record is 14 bytes, and is made up of 7 16-bit
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
/// The data in the WAD lump is structured as follows:
///
/// | Field Size | Data Type | Content                              |
/// |------------|-----------|--------------------------------------|
/// |  0x00-0x01 |    u16    | Index to vertex the line starts from |
/// |  0x02-0x03 |    u16    | Index to vertex the line ends with   |
/// |  0x04-0x05 |    u16    | Angle in Binary Angle Measurement (BAMS) |
/// |  0x06-0x07 |    u16    | Index to the linedef this seg travels along|
/// |  0x08-0x09 |    u16    | Direction along line. 0 == SEG is on the right and follows the line, 1 == SEG travels in opposite direction |
/// |  0x10-0x11 |    u16    | Offset: this is the distance along the linedef this seg starts at |
///
/// Each `Segment` record is 12 bytes
#[derive(Debug)]
pub struct Segment {
    /// The line starts from this point
    pub start_vertex: DPtr<Vertex>,
    /// The line ends at this point
    pub end_vertex: DPtr<Vertex>,
    /// Binary Angle Measurement
    ///
    /// Degrees(0-360) = angle * 0.005493164
    pub angle: f32,
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
        angle: f32,
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

    pub fn angle_to_degree(&self) -> f32 {
        self.angle * 0.005493164
    }
}

/// A `SubSector` divides up all the SECTORS into convex polygons. They are then
/// referenced through the NODES resources. There will be (number of nodes) + 1.
///
/// The data in the WAD lump is structured as follows:
///
/// | Field Size | Data Type | Content                            |
/// |------------|-----------|------------------------------------|
/// |  0x00-0x01 |    u16    | How many segments line this sector |
/// |  0x02-0x03 |    u16    | Index to the starting segment      |
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
    pub kind: u16,
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
        kind: u16,
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
            kind,
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
