//! Ceiling movement thinker: raise, lower, crusher
use std::ptr::null_mut;

use crate::{
    d_thinker::{ActionF, Think, Thinker, ThinkerType},
    level_data::{
        Level,
        map_defs::{LineDef, Sector},
    },
    p_map_object::MapObject,
    p_specials::{find_highest_ceiling_surrounding, move_plane, PlaneResult},
    DPtr,
};

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
        .sectors()
        .iter()
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

        let thinker = MapObject::create_thinker(
            ThinkerType::CeilingMove(ceiling),
            ActionF::Think(CeilingMove::think),
        );

        if let Some(ptr) = level.thinkers.push::<CeilingMove>(thinker) {
            unsafe {
                (*ptr).set_obj_thinker_ptr::<CeilingMove>(ptr);
                sec.specialdata = Some(ptr);
            }
        }
    }

    ret
}

impl Think for CeilingMove {
    fn think(object: &mut ThinkerType, level: &mut Level) -> bool {
        let ceiling = object.bad_mut::<CeilingMove>();

        if level.level_time & 7 == 0 && !matches!(ceiling.kind, CeilingKind::SilentCrushAndRaise) {
            // TODO: S_StartSound(&ceiling->sector->soundorg, sfx_stnmov);
        }

        match ceiling.direction {
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
                            (*ceiling.thinker).set_action(ActionF::Remove);
                        },
                        CeilingKind::CrushAndRaise | CeilingKind::FastCrushAndRaise => {
                            ceiling.direction = -1;
                        }
                        CeilingKind::SilentCrushAndRaise => {
                            // TODO: S_StartSound(&ceiling->sector->soundorg, sfx_pstop);
                            ceiling.direction = -1;
                        }
                        _ => {}
                    }
                }
            }
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
                            (*ceiling.thinker).set_action(ActionF::Remove);
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
                            //TODO: S_StartSound(&ceiling->sector->soundorg, sfx_pstop);
                        }
                        _ => {}
                    }
                } else if matches!(res, PlaneResult::Crushed) {
                    match ceiling.kind {
                        CeilingKind::SilentCrushAndRaise
                        | CeilingKind::CrushAndRaise
                        | CeilingKind::LowerAndCrush => {
                            ceiling.speed /= 8.0;
                        }
                        _ => {}
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

    fn thinker(&self) -> *mut Thinker {
        self.thinker
    }
}
