//! Ceiling movement thinker: raise, lower, crusher
//!
//! Doom source name `p_ceiling`
use std::ptr::null_mut;

use sound_traits::SfxName;

use crate::level::map_defs::{LineDef, Sector};
use crate::level::Level;
use crate::thing::MapObject;
use crate::thinker::{Think, Thinker, ThinkerData};
use crate::MapPtr;

use crate::env::specials::{find_highest_ceiling_surrounding, move_plane, PlaneResult};
use crate::env::switch::start_sector_sound;

const CEILSPEED: f32 = 1.0;

#[derive(Debug, Clone, Copy)]
pub enum CeilKind {
    LowerToFloor,
    RaiseToHighest,
    LowerAndCrush,
    CrushAndRaise,
    FastCrushAndRaise,
    SilentCrushAndRaise,
}

pub struct CeilingMove {
    pub thinker: *mut Thinker,
    pub sector: MapPtr<Sector>,
    pub kind: CeilKind,
    pub bottomheight: f32,
    pub topheight: f32,
    pub speed: f32,
    pub crush: bool,
    // 1 = up, 0 = waiting, -1 = down
    pub direction: i32,
    // ID
    pub tag: i16,
    pub olddirection: i32,
}

// TODO: track activeceilings

/// EV_DoFloor
pub fn ev_do_ceiling(line: MapPtr<LineDef>, kind: CeilKind, level: &mut Level) -> bool {
    let mut ret = false;

    if matches!(
        kind,
        CeilKind::FastCrushAndRaise | CeilKind::SilentCrushAndRaise | CeilKind::CrushAndRaise
    ) {
        // TODO: P_ActivateInStasisCeiling(line);
    }

    for sector in level
        .map_data
        .sectors_mut()
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        if sector.specialdata.is_some() {
            continue;
        }

        // Because we need to break lifetimes...
        let mut sec = MapPtr::new(sector);

        let mut ceiling = CeilingMove {
            thinker: null_mut(),
            sector: MapPtr::new(sector),
            kind,
            speed: CEILSPEED,
            crush: false,
            direction: 0,
            bottomheight: 0.0,
            topheight: 0.0,
            tag: sec.tag,
            olddirection: 0,
        };

        match kind {
            CeilKind::LowerToFloor => {
                ceiling.crush = true;
                ceiling.topheight = sec.ceilingheight;
                ceiling.bottomheight = sec.floorheight;
                ceiling.direction = -1;
            }
            CeilKind::RaiseToHighest => {
                ceiling.topheight = find_highest_ceiling_surrounding(sec.clone());
                ceiling.bottomheight = sec.floorheight;
                ceiling.direction = 1;
            }
            CeilKind::LowerAndCrush => {
                ceiling.crush = true;
                ceiling.topheight = sec.ceilingheight;
                ceiling.bottomheight = sec.floorheight + 8.0;
                ceiling.direction = -1;
            }
            CeilKind::CrushAndRaise | CeilKind::SilentCrushAndRaise => {
                ceiling.crush = true;
                ceiling.topheight = sec.ceilingheight;
                ceiling.bottomheight = sec.floorheight + 8.0;
                ceiling.direction = -1;
            }
            CeilKind::FastCrushAndRaise => {
                ceiling.crush = true;
                ceiling.topheight = sec.ceilingheight;
                ceiling.bottomheight = sec.floorheight + 8.0;
                ceiling.direction = -1;
                ceiling.speed *= 2.0;
            }
        }

        ret = true;

        let thinker =
            MapObject::create_thinker(ThinkerData::CeilingMove(ceiling), CeilingMove::think);

        if let Some(ptr) = level.thinkers.push::<CeilingMove>(thinker) {
            ptr.set_obj_thinker_ptr();
            sec.specialdata = Some(ptr);
        }
    }

    ret
}

impl Think for CeilingMove {
    fn think(object: &mut Thinker, level: &mut Level) -> bool {
        let ceiling = object.ceiling_mut();
        #[cfg(feature = "null_check")]
        if object.ceiling.is_null() {
            std::panic!("ceiling thinker was null");
        }
        let line = ceiling.sector.lines[0].as_ref();

        if level.level_time & 7 == 0 && !matches!(ceiling.kind, CeilKind::SilentCrushAndRaise) {
            start_sector_sound(line, SfxName::Stnmov, &level.snd_command);
        }

        match ceiling.direction {
            // UP
            1 => {
                let res = move_plane(
                    ceiling.sector.clone(),
                    ceiling.speed,
                    ceiling.topheight,
                    false,
                    1,
                    ceiling.direction,
                );

                if matches!(res, PlaneResult::PastDest) {
                    match ceiling.kind {
                        CeilKind::RaiseToHighest => unsafe {
                            ceiling.sector.specialdata = None;
                            (*ceiling.thinker).mark_remove();
                        },
                        CeilKind::CrushAndRaise | CeilKind::FastCrushAndRaise => {
                            ceiling.direction = -1;
                        }
                        CeilKind::SilentCrushAndRaise => {
                            start_sector_sound(line, SfxName::Pstop, &level.snd_command);
                            ceiling.direction = -1;
                        }
                        _ => {}
                    }
                }
            }
            // DOWN
            -1 => {
                let res = move_plane(
                    ceiling.sector.clone(),
                    ceiling.speed,
                    ceiling.bottomheight,
                    ceiling.crush,
                    1,
                    ceiling.direction,
                );

                if matches!(res, PlaneResult::PastDest) {
                    match ceiling.kind {
                        CeilKind::LowerToFloor | CeilKind::LowerAndCrush => unsafe {
                            ceiling.sector.specialdata = None;
                            (*ceiling.thinker).mark_remove();
                        },
                        CeilKind::CrushAndRaise => {
                            ceiling.speed = CEILSPEED;
                            ceiling.direction = 1;
                        }
                        CeilKind::FastCrushAndRaise => {
                            ceiling.speed = CEILSPEED;
                            ceiling.direction = 1;
                        }
                        CeilKind::SilentCrushAndRaise => {
                            ceiling.speed = CEILSPEED;
                            ceiling.direction = 1;
                            start_sector_sound(line, SfxName::Pstop, &level.snd_command);
                        }
                        _ => {}
                    }
                } else if matches!(res, PlaneResult::Crushed) {
                    match ceiling.kind {
                        CeilKind::SilentCrushAndRaise
                        | CeilKind::CrushAndRaise
                        | CeilKind::LowerAndCrush => {
                            ceiling.speed = 0.2;
                        }
                        _ => ceiling.speed = CEILSPEED,
                    }
                }
            }
            _ => {}
        }

        true
    }

    fn set_thinker_ptr(&mut self, ptr: *mut Thinker) {
        self.thinker = ptr;
    }

    fn thinker_mut(&mut self) -> &mut Thinker {
        #[cfg(feature = "null_check")]
        if self.thinker.is_null() {
            std::panic!("ceiling thinker was null");
        }
        unsafe { &mut *self.thinker }
    }

    fn thinker(&self) -> &Thinker {
        #[cfg(feature = "null_check")]
        if self.thinker.is_null() {
            std::panic!("ceiling thinker was null");
        }
        unsafe { &*self.thinker }
    }
}
