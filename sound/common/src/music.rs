pub const EPISODE4_MUS: [MusTrack; 9] = [
    MusTrack::E3M4, // American   e4m1
    MusTrack::E3M2, // Romero     e4m2
    MusTrack::E3M3, // Shawn      e4m3
    MusTrack::E1M5, // American   e4m4
    MusTrack::E2M7, // Tim        e4m5
    MusTrack::E2M4, // Romero     e4m6
    MusTrack::E2M6, // J.Anderson e4m7 CHIRON.WAD
    MusTrack::E2M5, // Shawn      e4m8
    MusTrack::E1M9, // Tim        e4m9
];

/// `#[repr(u8)]` is load-bearing: the enum is converted to/from `u8` via
/// `TryFrom<u8>` for direct discriminant arithmetic against episode/map.
/// Variants must remain in track ordering with no gaps; new variants must
/// be appended just before `NumMus`.
#[repr(u8)]
#[derive(Debug, PartialOrd, PartialEq, Eq, Ord, Copy, Clone)]
#[allow(non_camel_case_types)]
#[derive(Default)]
pub enum MusTrack {
    #[default]
    None,
    E1M1,
    E1M2,
    E1M3,
    E1M4,
    E1M5,
    E1M6,
    E1M7,
    E1M8,
    E1M9,
    E2M1,
    E2M2,
    E2M3,
    E2M4,
    E2M5,
    E2M6,
    E2M7,
    E2M8,
    E2M9,
    E3M1,
    E3M2,
    E3M3,
    E3M4,
    E3M5,
    E3M6,
    E3M7,
    E3M8,
    E3M9,
    Inter,
    Intro,
    Bunny,
    Victor,
    Introa,
    Runnin,
    Stalks,
    Countd,
    Betwee,
    Doom,
    The_Da,
    Shawn,
    Ddtblu,
    In_Cit,
    Dead,
    Stlks2,
    Theda2,
    Doom2,
    Ddtbl2,
    Runni2,
    Dead2,
    Stlks3,
    Romero,
    Shawn2,
    Messag,
    Count2,
    Ddtbl3,
    Ampie,
    Theda3,
    Adrian,
    Messg2,
    Romer2,
    Tense,
    Shawn3,
    Openin,
    Evil,
    Ultima,
    Read_M,
    Dm2ttl,
    Dm2int,
    NumMus,
}

impl MusTrack {
    pub fn lump_name(self) -> String {
        match self {
            Self::None | Self::NumMus => String::new(),
            other => format!("D_{:?}", other).to_ascii_uppercase(),
        }
    }
}

impl TryFrom<u8> for MusTrack {
    type Error = u8;

    fn try_from(i: u8) -> Result<Self, Self::Error> {
        if i >= MusTrack::NumMus as u8 {
            return Err(i);
        }
        // SAFETY: `MusTrack` is `#[repr(u8)]` with contiguous discriminants
        // `0..NumMus`; the bound check above guarantees `i` maps to a valid
        // variant.
        Ok(unsafe { std::mem::transmute::<u8, MusTrack>(i) })
    }
}
