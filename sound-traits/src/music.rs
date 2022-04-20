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

pub const EPISODE4_MUS: [MusEnum; 9] = [
    MusEnum::e3m4, // American	e4m1
    MusEnum::e3m2, // Romero	e4m2
    MusEnum::e3m3, // Shawn	e4m3
    MusEnum::e1m5, // American	e4m4
    MusEnum::e2m7, // Tim 	e4m5
    MusEnum::e2m4, // Romero	e4m6
    MusEnum::e2m6, // J.Anderson	e4m7 CHIRON.WAD
    MusEnum::e2m5, // Shawn	e4m8
    MusEnum::e1m9, // Tim		e4m9
];

#[derive(Debug, PartialOrd, PartialEq, Copy, Clone)]
pub enum MusEnum {
    None,
    e1m1,
    e1m2,
    e1m3,
    e1m4,
    e1m5,
    e1m6,
    e1m7,
    e1m8,
    e1m9,
    e2m1,
    e2m2,
    e2m3,
    e2m4,
    e2m5,
    e2m6,
    e2m7,
    e2m8,
    e2m9,
    e3m1,
    e3m2,
    e3m3,
    e3m4,
    e3m5,
    e3m6,
    e3m7,
    e3m8,
    e3m9,
    inter,
    intro,
    bunny,
    victor,
    introa,
    runnin,
    stalks,
    countd,
    betwee,
    doom,
    the_da,
    shawn,
    ddtblu,
    in_cit,
    dead,
    stlks2,
    theda2,
    doom2,
    ddtbl2,
    runni2,
    dead2,
    stlks3,
    romero,
    shawn2,
    messag,
    count2,
    ddtbl3,
    ampie,
    theda3,
    adrian,
    messg2,
    romer2,
    tense,
    shawn3,
    openin,
    evil,
    ultima,
    read_m,
    dm2ttl,
    dm2int,
    NumMus,
}

impl Default for MusEnum {
    fn default() -> Self {
        Self::None
    }
}

impl From<u8> for MusEnum {
    fn from(i: u8) -> Self {
        if i >= MusEnum::NumMus as u8 {
            panic!("{} is not a variant of MusEnum", i);
        }
        unsafe { std::mem::transmute(i) }
    }
}
