//! Doom source name `p_lights`

use std::ptr::null_mut;

use crate::{
    level_data::{
        map_defs::{LineDef, Sector},
        Level,
    },
    DPtr,
};

use super::{
    d_thinker::{ObjectType, Think, Thinker},
    map_object::MapObject,
    specials::{find_max_light_surrounding, find_min_light_surrounding, get_next_sector},
    utilities::p_random,
};

const STROBEBRIGHT: i32 = 5;
pub const FASTDARK: i32 = 15;
pub const SLOWDARK: i32 = 35;

pub struct FireFlicker {
    pub thinker: *mut Thinker,
    pub sector: DPtr<Sector>,
    pub count: i32,
    pub max_light: i32,
    pub min_light: i32,
}

impl FireFlicker {
    /// Doom function name `P_SpawnFireFlicker`
    pub fn spawn(sector: &mut Sector, level: &mut Level) {
        sector.special = 0;
        let light = FireFlicker {
            thinker: null_mut(),
            sector: DPtr::new(sector),
            count: 4,
            max_light: sector.lightlevel,
            min_light: find_min_light_surrounding(DPtr::new(sector), sector.lightlevel) + 16,
        };

        let thinker = MapObject::create_thinker(ObjectType::FireFlicker(light), FireFlicker::think);

        if let Some(ptr) = level.thinkers.push::<FireFlicker>(thinker) {
            unsafe {
                (*ptr).set_obj_thinker_ptr::<FireFlicker>(ptr);
            }
        }
    }
}

impl Think for FireFlicker {
    fn think(object: &mut ObjectType, _level: &mut Level) -> bool {
        let mut light = object.bad_mut::<FireFlicker>();
        light.count -= 1;
        if light.count != 0 {
            return false;
        }

        let amount = (p_random() & 3) * 16;
        if light.sector.lightlevel - amount < light.min_light {
            light.sector.lightlevel = light.min_light
        } else {
            light.sector.lightlevel = light.max_light - amount;
        }
        light.count = 4;

        false
    }

    fn set_thinker_ptr(&mut self, ptr: *mut Thinker) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> *mut Thinker {
        self.thinker
    }
}

pub struct LightFlash {
    pub thinker: *mut Thinker,
    pub sector: DPtr<Sector>,
    pub count: i32,
    pub max_light: i32,
    pub min_light: i32,
    pub max_time: i32,
    pub min_time: i32,
}

impl LightFlash {
    /// Doom function name `P_SpawnLightFlash`
    pub fn spawn(sector: &mut Sector, level: &mut Level) {
        sector.special = 0;
        let light = LightFlash {
            thinker: null_mut(),
            sector: DPtr::new(sector),
            count: (p_random() & 64) + 1,
            max_light: sector.lightlevel,
            min_light: find_min_light_surrounding(DPtr::new(sector), sector.lightlevel),
            max_time: 64,
            min_time: 7,
        };

        let thinker = MapObject::create_thinker(ObjectType::LightFlash(light), LightFlash::think);

        if let Some(ptr) = level.thinkers.push::<LightFlash>(thinker) {
            unsafe {
                (*ptr).set_obj_thinker_ptr::<LightFlash>(ptr);
            }
        }
    }
}

impl Think for LightFlash {
    fn think(object: &mut ObjectType, _level: &mut Level) -> bool {
        let mut light = object.bad_mut::<LightFlash>();
        light.count -= 1;
        if light.count != 0 {
            return false;
        }

        if light.sector.lightlevel == light.max_light {
            light.sector.lightlevel = light.min_light;
            light.count = (p_random() & light.min_time) + 1
        } else {
            light.sector.lightlevel = light.max_light;
            light.count = (p_random() & light.max_time) + 1
        }

        false
    }

    fn set_thinker_ptr(&mut self, ptr: *mut Thinker) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> *mut Thinker {
        self.thinker
    }
}

pub struct StrobeFlash {
    pub thinker: *mut Thinker,
    pub sector: DPtr<Sector>,
    pub count: i32,
    pub min_light: i32,
    pub max_light: i32,
    pub dark_time: i32,
    pub bright_time: i32,
}

impl StrobeFlash {
    /// Doom function name `P_SpawnStrobeFlash`
    pub fn spawn(sector: &mut Sector, fast_or_slow: i32, in_sync: bool, level: &mut Level) {
        sector.special = 0;
        let mut light = StrobeFlash {
            thinker: null_mut(),
            sector: DPtr::new(sector),
            count: if in_sync { (p_random() & 7) + 1 } else { 1 },
            min_light: find_min_light_surrounding(DPtr::new(sector), sector.lightlevel),
            max_light: sector.lightlevel,
            dark_time: fast_or_slow,
            bright_time: STROBEBRIGHT,
        };

        if light.min_light == light.max_light {
            light.min_light = 0;
        }

        let thinker = MapObject::create_thinker(ObjectType::StrobeFlash(light), StrobeFlash::think);

        if let Some(ptr) = level.thinkers.push::<StrobeFlash>(thinker) {
            unsafe {
                (*ptr).set_obj_thinker_ptr::<StrobeFlash>(ptr);
            }
        }
    }
}

impl Think for StrobeFlash {
    fn think(object: &mut ObjectType, _level: &mut Level) -> bool {
        let mut light = object.bad_mut::<StrobeFlash>();
        light.count -= 1;
        if light.count != 0 {
            return false;
        }

        if light.sector.lightlevel == light.min_light {
            light.sector.lightlevel = light.max_light;
            light.count = light.bright_time;
        } else {
            light.sector.lightlevel = light.min_light;
            light.count = light.dark_time;
        }

        false
    }

    fn set_thinker_ptr(&mut self, ptr: *mut Thinker) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> *mut Thinker {
        self.thinker
    }
}

pub struct Glow {
    pub thinker: *mut Thinker,
    pub sector: DPtr<Sector>,
    pub min_light: i32,
    pub max_light: i32,
    pub direction: i32,
}

impl Glow {
    /// Doom function name `P_SpawnGlowingLight`
    pub fn spawn(sector: &mut Sector, level: &mut Level) {
        sector.special = 0;
        let light = Glow {
            thinker: null_mut(),
            sector: DPtr::new(sector),
            max_light: sector.lightlevel,
            min_light: find_min_light_surrounding(DPtr::new(sector), sector.lightlevel),
            direction: -1,
        };

        let thinker = MapObject::create_thinker(ObjectType::Glow(light), Glow::think);

        if let Some(ptr) = level.thinkers.push::<Glow>(thinker) {
            unsafe {
                (*ptr).set_obj_thinker_ptr::<Glow>(ptr);
            }
        }
    }
}

const GLOWSPEED: i32 = 8;

impl Think for Glow {
    fn think(object: &mut ObjectType, _level: &mut Level) -> bool {
        let mut light = object.bad_mut::<Glow>();
        match light.direction {
            -1 => {
                light.sector.lightlevel -= GLOWSPEED;
                if light.sector.lightlevel <= light.min_light {
                    light.sector.lightlevel += GLOWSPEED;
                    light.direction = 1;
                }
            }
            1 => {
                light.sector.lightlevel += GLOWSPEED;
                if light.sector.lightlevel >= light.max_light {
                    light.sector.lightlevel -= GLOWSPEED;
                    light.direction = -1;
                }
            }
            _ => {}
        }

        false
    }

    fn set_thinker_ptr(&mut self, ptr: *mut Thinker) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> *mut Thinker {
        self.thinker
    }
}

/// Doom function name `EV_LightTurnOn`
pub fn ev_turn_light_on(line: DPtr<LineDef>, mut bright: i32, level: &mut Level) {
    for sector in level
        .map_data
        .sectors
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        // Because we need to break lifetimes...
        let sec = DPtr::new(sector);

        // bright = 0 means to search
        // for highest light level
        // surrounding sector
        if bright == 0 {
            bright = find_max_light_surrounding(sec, bright);
        }
        sector.lightlevel = bright;
    }
}

/// Doom function name `EV_TurnTagLightsOff`
pub fn ev_turn_tag_lights_off(line: DPtr<LineDef>, level: &mut Level) {
    let mut min;
    for sector in level
        .map_data
        .sectors
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        let sec = DPtr::new(sector);
        min = sector.lightlevel;

        for line in sector.lines.iter_mut() {
            let tsec = get_next_sector(line.clone(), sec.clone());
            if let Some(tsec) = tsec {
                if tsec.lightlevel < min {
                    min = tsec.lightlevel;
                }
            }
        }

        sector.lightlevel = min;
    }
}

/// Doom function name `EV_StartLightStrobing`
pub fn ev_start_light_strobing(line: DPtr<LineDef>, level: &mut Level) {
    let level_ptr = unsafe { &mut *(level as *mut Level) };
    for sector in level
        .map_data
        .sectors
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        if sector.specialdata.is_none() {
            StrobeFlash::spawn(sector, SLOWDARK, false, level_ptr);
        }
    }
}
