//! Doom source name `p_lights`

use std::ptr::null_mut;

use crate::level::map_defs::{LineDef, Sector};
use crate::level::Level;
use crate::thing::MapObject;
use crate::thinker::{Think, Thinker, ThinkerData};
use crate::MapPtr;

use crate::env::specials::{
    find_max_light_surrounding, find_min_light_surrounding, get_next_sector
};
use math::p_random;

const STROBEBRIGHT: i32 = 5;
pub const FASTDARK: i32 = 15;
pub const SLOWDARK: i32 = 35;

// pub struct MergedLight {
//     // Comon
//     pub thinker: *mut Thinker,
//     pub sector: DPtr<Sector>,
//     pub count: i32,
//     pub max_light: i32,
//     pub min_light: i32,
//     // Specialised
//     pub max_time: i32,
//     pub min_time: i32,
//     pub dark_time: i32,
//     pub bright_time: i32,
//     pub direction: i32,
// }

pub struct FireFlicker {
    pub thinker: *mut Thinker,
    pub sector: MapPtr<Sector>,
    pub count: i32,
    pub max_light: usize,
    pub min_light: usize,
}

impl FireFlicker {
    /// Doom function name `P_SpawnFireFlicker`
    pub fn spawn(sector: &mut Sector, level: &mut Level) {
        sector.special = 0;
        let light = FireFlicker {
            thinker: null_mut(),
            sector: MapPtr::new(sector),
            count: 4,
            max_light: sector.lightlevel,
            min_light: find_min_light_surrounding(MapPtr::new(sector), sector.lightlevel) + 16,
        };

        let thinker =
            MapObject::create_thinker(ThinkerData::FireFlicker(light), FireFlicker::think);

        if let Some(ptr) = level.thinkers.push::<FireFlicker>(thinker) {
            ptr.set_obj_thinker_ptr();
        }
    }
}

impl Think for FireFlicker {
    fn think(object: &mut Thinker, _level: &mut Level) -> bool {
        let light = object.fire_flick_mut();
        #[cfg(feature = "null_check")]
        if light.thinker.is_null() {
            std::panic!("fire flicker thinker was null");
        }

        light.count -= 1;
        if light.count != 0 {
            return false;
        }

        let amount = ((p_random() & 3) * 16) as usize;
        if light.sector.lightlevel >= amount {
            light.sector.lightlevel = light.max_light - amount;
        }
        if light.sector.lightlevel < light.min_light {
            light.sector.lightlevel = light.min_light
        }
        light.count = 4;

        false
    }

    fn set_thinker_ptr(&mut self, ptr: *mut Thinker) {
        self.thinker = ptr;
    }

    fn thinker_mut(&mut self) -> &mut Thinker {
        #[cfg(feature = "null_check")]
        if self.thinker.is_null() {
            std::panic!("fire flicker thinker was null");
        }
        unsafe { &mut *self.thinker }
    }

    fn thinker(&self) -> &Thinker {
        #[cfg(feature = "null_check")]
        if self.thinker.is_null() {
            std::panic!("fire flicker thinker was null");
        }
        unsafe { &*self.thinker }
    }
}

pub struct LightFlash {
    pub thinker: *mut Thinker,
    pub sector: MapPtr<Sector>,
    pub count: i32,
    pub max_light: usize,
    pub min_light: usize,
    pub max_time: i32,
    pub min_time: i32,
}

impl LightFlash {
    /// Doom function name `P_SpawnLightFlash`
    pub fn spawn(sector: &mut Sector, level: &mut Level) {
        sector.special = 0;
        let light = LightFlash {
            thinker: null_mut(),
            sector: MapPtr::new(sector),
            count: (p_random() & 64) + 1,
            max_light: sector.lightlevel,
            min_light: find_min_light_surrounding(MapPtr::new(sector), sector.lightlevel),
            max_time: 64,
            min_time: 7,
        };

        let thinker = MapObject::create_thinker(ThinkerData::LightFlash(light), LightFlash::think);

        if let Some(ptr) = level.thinkers.push::<LightFlash>(thinker) {
            ptr.set_obj_thinker_ptr();
        }
    }
}

impl Think for LightFlash {
    fn think(object: &mut Thinker, _level: &mut Level) -> bool {
        let light = object.light_flash_mut();
        #[cfg(feature = "null_check")]
        if light.thinker.is_null() {
            std::panic!("light flash thinker was null");
        }

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

    fn thinker_mut(&mut self) -> &mut Thinker {
        #[cfg(feature = "null_check")]
        if self.thinker.is_null() {
            std::panic!("light flash thinker was null");
        }
        unsafe { &mut *self.thinker }
    }

    fn thinker(&self) -> &Thinker {
        #[cfg(feature = "null_check")]
        if self.thinker.is_null() {
            std::panic!("light flash thinker was null");
        }
        unsafe { &*self.thinker }
    }
}

pub struct StrobeFlash {
    pub thinker: *mut Thinker,
    pub sector: MapPtr<Sector>,
    pub count: i32,
    pub min_light: usize,
    pub max_light: usize,
    pub dark_time: i32,
    pub bright_time: i32,
}

impl StrobeFlash {
    /// Doom function name `P_SpawnStrobeFlash`
    pub fn spawn(sector: &mut Sector, fast_or_slow: i32, in_sync: bool, level: &mut Level) {
        sector.special = 0;
        let mut light = StrobeFlash {
            thinker: null_mut(),
            sector: MapPtr::new(sector),
            count: if in_sync { (p_random() & 7) + 1 } else { 1 },
            min_light: find_min_light_surrounding(MapPtr::new(sector), sector.lightlevel),
            max_light: sector.lightlevel,
            dark_time: fast_or_slow,
            bright_time: STROBEBRIGHT,
        };

        if light.min_light == light.max_light {
            light.min_light = 0;
        }

        let thinker =
            MapObject::create_thinker(ThinkerData::StrobeFlash(light), StrobeFlash::think);

        if let Some(ptr) = level.thinkers.push::<StrobeFlash>(thinker) {
            ptr.set_obj_thinker_ptr();
        }
    }
}

impl Think for StrobeFlash {
    fn think(object: &mut Thinker, _level: &mut Level) -> bool {
        let light = object.strobe_flash_mut();
        #[cfg(feature = "null_check")]
        if light.thinker.is_null() {
            std::panic!("strobe flash thinker was null");
        }

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

    fn thinker_mut(&mut self) -> &mut Thinker {
        #[cfg(feature = "null_check")]
        if self.thinker.is_null() {
            std::panic!("strobe flash thinker was null");
        }
        unsafe { &mut *self.thinker }
    }

    fn thinker(&self) -> &Thinker {
        #[cfg(feature = "null_check")]
        if self.thinker.is_null() {
            std::panic!("strobe flash thinker was null");
        }
        unsafe { &*self.thinker }
    }
}

pub struct Glow {
    pub thinker: *mut Thinker,
    pub sector: MapPtr<Sector>,
    pub min_light: usize,
    pub max_light: usize,
    pub direction: i32,
}

impl Glow {
    /// Doom function name `P_SpawnGlowingLight`
    pub fn spawn(sector: &mut Sector, level: &mut Level) {
        sector.special = 0;
        let light = Glow {
            thinker: null_mut(),
            sector: MapPtr::new(sector),
            max_light: sector.lightlevel,
            min_light: find_min_light_surrounding(MapPtr::new(sector), sector.lightlevel),
            direction: -1,
        };

        let thinker = MapObject::create_thinker(ThinkerData::Glow(light), Glow::think);

        if let Some(ptr) = level.thinkers.push::<Glow>(thinker) {
            ptr.set_obj_thinker_ptr();
        }
    }
}

const GLOWSPEED: usize = 8;

impl Think for Glow {
    fn think(object: &mut Thinker, _level: &mut Level) -> bool {
        let light = object.glow_mut();
        #[cfg(feature = "null_check")]
        if light.thinker.is_null() {
            std::panic!("glow thinker was null");
        }

        match light.direction {
            -1 => {
                if light.sector.lightlevel >= GLOWSPEED {
                    light.sector.lightlevel -= GLOWSPEED;
                }
                if light.sector.lightlevel <= light.min_light {
                    light.sector.lightlevel += GLOWSPEED;
                    light.direction = 1;
                }
            }
            1 => {
                light.sector.lightlevel += GLOWSPEED;
                if light.sector.lightlevel >= light.max_light {
                    if light.sector.lightlevel >= GLOWSPEED {
                        light.sector.lightlevel -= GLOWSPEED;
                    }
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

    fn thinker_mut(&mut self) -> &mut Thinker {
        #[cfg(feature = "null_check")]
        if self.thinker.is_null() {
            std::panic!("glow thinker was null");
        }
        unsafe { &mut *self.thinker }
    }

    fn thinker(&self) -> &Thinker {
        #[cfg(feature = "null_check")]
        if self.thinker.is_null() {
            std::panic!("glow thinker was null");
        }
        unsafe { &*self.thinker }
    }
}

/// Doom function name `EV_LightTurnOn`
pub fn ev_turn_light_on(line: MapPtr<LineDef>, mut bright: usize, level: &mut Level) {
    for sector in level
        .map_data
        .sectors_mut()
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        // Because we need to break lifetimes...
        let sec = MapPtr::new(sector);

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
pub fn ev_turn_tag_lights_off(line: MapPtr<LineDef>, level: &mut Level) {
    let mut min;
    for sector in level
        .map_data
        .sectors_mut()
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        let sec = MapPtr::new(sector);
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
pub fn ev_start_light_strobing(line: MapPtr<LineDef>, level: &mut Level) {
    let level_ptr = unsafe { &mut *(level as *mut Level) };
    for sector in level
        .map_data
        .sectors_mut()
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        if sector.specialdata.is_none() {
            StrobeFlash::spawn(sector, SLOWDARK, false, level_ptr);
        }
    }
}
