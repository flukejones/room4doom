//! Ceiling movement thinker: raise, lower, crusher
//!
//! Doom source name `p_ceiling`
use std::ptr::null_mut;

use sound_traits::SfxEnum;

use crate::obj::MapObject;
use crate::{
    level::{
        map_defs::{LineDef, Sector},
        Level,
    },
    thinker::{Think, Thinker, ThinkerData},
    DPtr,
};

use crate::env::specials::{find_highest_ceiling_surrounding, move_plane, PlaneResult};
use crate::env::switch::start_sector_sound;

const CEILSPEED: f32 = 1.0;

#[derive(Debug, Clone, Copy)]
pub enum CeilingKind {
    LowerToFloor,
    RaiseToHighest,
    LowerAndCrush,
    CrushAndRaise,
    FastCrushAndRaise,
    SilentCrushAndRaise,
}

pub struct CeilingMove {
    pub thinker: *mut Thinker,
    pub sector: DPtr<Sector>,
    pub kind: CeilingKind,
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
pub fn ev_do_ceiling(line: DPtr<LineDef>, kind: CeilingKind, level: &mut Level) -> bool {
    let mut ret = false;

    if matches!(
        kind,
        CeilingKind::FastCrushAndRaise
            | CeilingKind::SilentCrushAndRaise
            | CeilingKind::CrushAndRaise
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
        let mut sec = DPtr::new(sector);

        let mut ceiling = CeilingMove {
            thinker: null_mut(),
            sector: DPtr::new(sector),
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
            CeilingKind::LowerToFloor => {
                ceiling.crush = true;
                ceiling.topheight = sec.ceilingheight;
                ceiling.bottomheight = sec.floorheight;
                ceiling.direction = -1;
            }
            CeilingKind::RaiseToHighest => {
                ceiling.topheight = find_highest_ceiling_surrounding(sec.clone());
                ceiling.bottomheight = sec.floorheight;
                ceiling.direction = 1;
            }
            CeilingKind::LowerAndCrush => {
                ceiling.crush = true;
                ceiling.topheight = sec.ceilingheight;
                ceiling.bottomheight = sec.floorheight + 8.0;
                ceiling.direction = -1;
            }
            CeilingKind::CrushAndRaise | CeilingKind::SilentCrushAndRaise => {
                ceiling.crush = true;
                ceiling.topheight = sec.ceilingheight;
                ceiling.bottomheight = sec.floorheight + 8.0;
                ceiling.direction = -1;
            }
            CeilingKind::FastCrushAndRaise => {
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
        #[cfg(null_check)]
        if self.ceiling.is_null() {
            std::panic!("ceiling thinker was null");
        }
        let line = ceiling.sector.lines[0].as_ref();

        if level.level_time & 7 == 0 && !matches!(ceiling.kind, CeilingKind::SilentCrushAndRaise) {
            start_sector_sound(line, SfxEnum::stnmov, &level.snd_command);
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
                        CeilingKind::RaiseToHighest => unsafe {
                            ceiling.sector.specialdata = None;
                            (*ceiling.thinker).mark_remove();
                        },
                        CeilingKind::CrushAndRaise | CeilingKind::FastCrushAndRaise => {
                            ceiling.direction = -1;
                        }
                        CeilingKind::SilentCrushAndRaise => {
                            start_sector_sound(line, SfxEnum::pstop, &level.snd_command);
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
                        CeilingKind::LowerToFloor | CeilingKind::LowerAndCrush => unsafe {
                            ceiling.sector.specialdata = None;
                            (*ceiling.thinker).mark_remove();
                        },
                        CeilingKind::CrushAndRaise => {
                            ceiling.speed = CEILSPEED;
                            ceiling.direction = 1;
                        }
                        CeilingKind::FastCrushAndRaise => {
                            ceiling.speed = CEILSPEED;
                            ceiling.direction = 1;
                        }
                        CeilingKind::SilentCrushAndRaise => {
                            ceiling.speed = CEILSPEED;
                            ceiling.direction = 1;
                            start_sector_sound(line, SfxEnum::pstop, &level.snd_command);
                        }
                        _ => {}
                    }
                } else if matches!(res, PlaneResult::Crushed) {
                    match ceiling.kind {
                        CeilingKind::SilentCrushAndRaise
                        | CeilingKind::CrushAndRaise
                        | CeilingKind::LowerAndCrush => {
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
        #[cfg(null_check)]
        if self.thinker.is_null() {
            std::panic!("ceiling thinker was null");
        }
        unsafe { &mut *self.thinker }
    }

    fn thinker(&self) -> &Thinker {
        #[cfg(null_check)]
        if self.thinker.is_null() {
            std::panic!("ceiling thinker was null");
        }
        unsafe { &*self.thinker }
    }
}
