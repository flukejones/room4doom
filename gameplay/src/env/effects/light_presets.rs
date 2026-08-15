//! Vanilla light specials expressed as [`LightEffect`] presets.
//!
//! Doom source name `p_lights`

use std::ptr::from_mut;

use super::{LightEffect, LightPattern, base_light, push_light};
use crate::level::LevelState;
use level::MapPtr;
use level::env_query::{find_max_light_surrounding, get_next_sector};
use level::map_defs::{LineDef, Sector};
use math::p_random;

/// Tics a strobe holds its bright phase.
const STROBEBRIGHT: i32 = 5;
/// Tics a fast strobe holds its dark phase.
const FASTDARK: i32 = 15;
/// Tics a slow strobe holds its dark phase.
const SLOWDARK: i32 = 35;
/// Random mask for the bright hold of a light flash.
const FLASH_BRIGHT_MASK: i32 = 64;
/// Random mask for the dark hold of a light flash.
const FLASH_DARK_MASK: i32 = 7;
/// Fire flicker sits this far above the darkest neighbour.
const FIRE_MIN_OFFSET: usize = 16;
/// First fire re-roll comes after this many tics.
const FIRE_FUSE: i32 = 4;

/// The vanilla light parameterisations, one per spawnable behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightPreset {
    Flash,
    StrobeFast,
    StrobeSlow,
    StrobeFastSync,
    StrobeSlowSync,
    Glow,
    Fire,
}

/// Build and insert the light thinker for `preset` on `sector`.
///
/// Doom function names `P_SpawnLightFlash`, `P_SpawnStrobeFlash`,
/// `P_SpawnFireFlicker`, `P_SpawnGlowingLight`.
pub fn spawn_light(sector: &mut Sector, preset: LightPreset, level: &mut LevelState) {
    let light = match preset {
        LightPreset::Flash => {
            let mut light = base_light(sector, LightPattern::Flash);
            light.countdown = (p_random() & FLASH_BRIGHT_MASK) + 1;
            light.dark_time = FLASH_DARK_MASK;
            light.bright_time = FLASH_BRIGHT_MASK;
            light
        }
        LightPreset::StrobeFast => strobe(sector, FASTDARK, false),
        LightPreset::StrobeSlow => strobe(sector, SLOWDARK, false),
        LightPreset::StrobeFastSync => strobe(sector, FASTDARK, true),
        LightPreset::StrobeSlowSync => strobe(sector, SLOWDARK, true),
        LightPreset::Glow => {
            let mut light = base_light(sector, LightPattern::Glow);
            light.direction = -1;
            light
        }
        LightPreset::Fire => {
            let mut light = base_light(sector, LightPattern::Fire);
            light.min_light += FIRE_MIN_OFFSET;
            light.countdown = FIRE_FUSE;
            light
        }
    };
    push_light(light, level);
}

fn strobe(sector: &mut Sector, dark_time: i32, in_sync: bool) -> LightEffect {
    let mut light = base_light(sector, LightPattern::Strobe);
    light.countdown = if in_sync { 1 } else { (p_random() & 7) + 1 };
    light.dark_time = dark_time;
    light.bright_time = STROBEBRIGHT;
    if light.min_light == light.max_light {
        light.min_light = 0;
    }
    light
}

/// Doom function name `EV_LightTurnOn`
pub fn ev_turn_light_on(line: MapPtr<LineDef>, mut bright: usize, level: &mut LevelState) {
    for sector in level
        .level_data
        .sectors
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        let sec = MapPtr::new(sector);

        // bright = 0 selects the brightest surrounding sector instead of a fixed level.
        if bright == 0 {
            bright = find_max_light_surrounding(sec, bright);
        }
        sector.lightlevel = bright;
    }
}

/// Doom function name `EV_TurnTagLightsOff`
pub fn ev_turn_tag_lights_off(line: MapPtr<LineDef>, level: &mut LevelState) {
    let mut min;
    for sector in level
        .level_data
        .sectors
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        let sec = MapPtr::new(sector);
        min = sector.lightlevel;

        for line in &mut sector.lines {
            let tsec = get_next_sector(line.clone(), sec.clone());
            if let Some(tsec) = tsec
                && tsec.lightlevel < min
            {
                min = tsec.lightlevel;
            }
        }

        sector.lightlevel = min;
    }
}

/// Doom function name `EV_StartLightStrobing`
pub fn ev_start_light_strobing(line: MapPtr<LineDef>, level: &mut LevelState) {
    // SAFETY: the level outlives every thinker spawned into it.
    let level_ptr = unsafe { &mut *from_mut::<LevelState>(level) };
    for sector in level
        .level_data
        .sectors
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        if sector.specialdata.is_none() {
            spawn_light(sector, LightPreset::StrobeSlow, level_ptr);
        }
    }
}
