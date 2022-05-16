#[derive(Debug)]
pub struct MusData {
    name: &'static str,
    data: Vec<u8>,
}

impl MusData {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            data: Vec::new(),
        }
    }

    pub fn lump_name(&self) -> String {
        format!("D_{}", self.name.to_uppercase())
    }

    pub fn set_data(&mut self, data: Vec<u8>) {
        self.data = data
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// Requires the user to initialise the data for each `MusData`. This is unsafe and
/// should be done as part of the startup code.
pub static mut MUS_DATA: [MusData; 68] = [
    MusData::new(""),
    MusData::new("e1m1"),
    MusData::new("e1m2"),
    MusData::new("e1m3"),
    MusData::new("e1m4"),
    MusData::new("e1m5"),
    MusData::new("e1m6"),
    MusData::new("e1m7"),
    MusData::new("e1m8"),
    MusData::new("e1m9"),
    MusData::new("e2m1"),
    MusData::new("e2m2"),
    MusData::new("e2m3"),
    MusData::new("e2m4"),
    MusData::new("e2m5"),
    MusData::new("e2m6"),
    MusData::new("e2m7"),
    MusData::new("e2m8"),
    MusData::new("e2m9"),
    MusData::new("e3m1"),
    MusData::new("e3m2"),
    MusData::new("e3m3"),
    MusData::new("e3m4"),
    MusData::new("e3m5"),
    MusData::new("e3m6"),
    MusData::new("e3m7"),
    MusData::new("e3m8"),
    MusData::new("e3m9"),
    MusData::new("inter"),
    MusData::new("intro"),
    MusData::new("bunny"),
    MusData::new("victor"),
    MusData::new("introa"),
    MusData::new("runnin"),
    MusData::new("stalks"),
    MusData::new("countd"),
    MusData::new("betwee"),
    MusData::new("doom"),
    MusData::new("the_da"),
    MusData::new("shawn"),
    MusData::new("ddtblu"),
    MusData::new("in_cit"),
    MusData::new("dead"),
    MusData::new("stlks2"),
    MusData::new("theda2"),
    MusData::new("doom2,"),
    MusData::new("ddtbl2"),
    MusData::new("runni2"),
    MusData::new("dead2"),
    MusData::new("stlks3"),
    MusData::new("romero"),
    MusData::new("shawn2"),
    MusData::new("messag"),
    MusData::new("count2"),
    MusData::new("ddtbl3"),
    MusData::new("ampie"),
    MusData::new("theda3"),
    MusData::new("adrian"),
    MusData::new("messg2"),
    MusData::new("romer2"),
    MusData::new("tense"),
    MusData::new("shawn3"),
    MusData::new("openin"),
    MusData::new("evil"),
    MusData::new("ultima"),
    MusData::new("read_m"),
    MusData::new("dm2ttl"),
    MusData::new("dm2int"),
];

pub const EPISODE4_MUS: [MusTrack; 9] = [
    MusTrack::E3M4, // American   e4m1
    MusTrack::E3M2, // Romero     e4m2
    MusTrack::E3M3, // Shawn      e4m3
    MusTrack::E1M5, // American   e4m4
    MusTrack::E2M7, // Tim        e4m5
    MusTrack::E2M4, // Romero	 e4m6
    MusTrack::E2M6, // J.Anderson e4m7 CHIRON.WAD
    MusTrack::E2M5, // Shawn      e4m8
    MusTrack::E1M9, // Tim        e4m9
];

#[derive(Debug, PartialOrd, PartialEq, Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum MusTrack {
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

impl Default for MusTrack {
    fn default() -> Self {
        Self::None
    }
}

impl From<u8> for MusTrack {
    fn from(i: u8) -> Self {
        if i >= MusTrack::NumMus as u8 {
            panic!("{} is not a variant of MusEnum", i);
        }
        unsafe { std::mem::transmute(i) }
    }
}
