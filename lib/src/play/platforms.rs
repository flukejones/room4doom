//! Platform movement thinker: raise and lower. Can be crushers and can be repeating movements.
//!
//! Doom source name `p_plats`

use std::ptr::null_mut;

use super::{
    d_thinker::{ObjectType, Think, Thinker},
    map_object::MapObject,
    specials::{
        find_highest_floor_surrounding, find_lowest_floor_surrounding, move_plane, PlaneResult,
    },
    utilities::p_random,
};

use crate::{
    doom_def::TICRATE,
    level::{
        map_defs::{LineDef, Sector},
        Level,
    },
    DPtr,
};

// TODO: active platform tracking? Seems to be required for "animated" platforms.

const PLATSPEED: f32 = 1.0;
const PLATWAIT: i32 = 3;

#[derive(Debug, Clone, Copy)]
pub enum PlatStatus {
    Up,
    Down,
    Waiting,
    InStasis,
}

#[derive(Debug, Clone, Copy)]
pub enum PlatKind {
    PerpetualRaise,
    DownWaitUpStay,
    RaiseAndChange,
    RaiseToNearestAndChange,
    BlazeDWUS,
}

pub struct Platform {
    pub thinker: *mut Thinker,
    pub sector: DPtr<Sector>,
    pub speed: f32,
    pub low: f32,
    pub high: f32,
    pub wait: i32,
    pub count: i32,
    pub status: PlatStatus,
    pub old_status: PlatStatus,
    pub crush: bool,
    pub tag: i16,
    pub kind: PlatKind,
}

pub fn ev_do_platform(line: DPtr<LineDef>, kind: PlatKind, amount: i32, level: &mut Level) -> bool {
    let mut ret = false;

    if matches!(kind, PlatKind::PerpetualRaise) {
        // TODO: P_ActivateInStasis(line->tag);
    }

    for sector in level
        .map_data
        .sectors()
        .iter()
        .filter(|s| s.tag == line.tag)
    {
        // TODO: track active platforms and reset sector special data
        if sector.specialdata.is_some() {
            continue;
        }
        ret = true;

        // Because we need to break lifetimes...
        let mut sec = DPtr::new(sector);

        let mut platform = Platform {
            thinker: null_mut(),
            sector: DPtr::new(sector),
            speed: PLATSPEED,
            low: 0.0,
            high: 0.0,
            wait: 0,
            count: 0,
            status: PlatStatus::InStasis,
            old_status: PlatStatus::InStasis,
            crush: false,
            tag: line.tag,
            kind,
        };

        match kind {
            PlatKind::RaiseToNearestAndChange => {
                platform.speed /= 2.0;
                platform.high = find_highest_floor_surrounding(sec.clone());
                platform.wait = 0;
                platform.status = PlatStatus::Up;
                sec.special = 0;
                sec.floorpic = line.frontsector.floorpic;
                // TODO: S_StartSound(&sec->soundorg, sfx_stnmov);
            }
            PlatKind::RaiseAndChange => {
                platform.speed /= 2.0;
                platform.high = sec.floorheight + amount as f32;
                platform.wait = 0;
                platform.status = PlatStatus::Up;
                sec.floorpic = line.frontsector.floorpic;
                // TODO: S_StartSound(&sec->soundorg, sfx_stnmov);
            }

            PlatKind::PerpetualRaise => {
                platform.low = find_lowest_floor_surrounding(sec.clone());

                if platform.low > sec.floorheight {
                    platform.low = sec.floorheight;
                }

                platform.high = find_highest_floor_surrounding(sec.clone());

                if platform.high < sec.floorheight {
                    platform.high = sec.floorheight;
                }

                platform.wait = TICRATE * PLATWAIT;

                platform.status = if (p_random() & 1) == 0 {
                    PlatStatus::Up
                } else {
                    PlatStatus::Down
                };
                // TODO: plat->status = P_Random() & 1;
                // TODO: S_StartSound(&sec->soundorg, sfx_pstart);
            }
            PlatKind::DownWaitUpStay => {
                platform.speed *= 4.0;
                platform.low = find_lowest_floor_surrounding(sec.clone());

                if platform.low > sec.floorheight {
                    platform.low = sec.floorheight;
                }

                platform.high = sec.floorheight;
                platform.wait = TICRATE * PLATWAIT;
                platform.status = PlatStatus::Down;
                // TODO: S_StartSound(&sec->soundorg, sfx_pstart);
            }
            PlatKind::BlazeDWUS => {
                platform.speed *= 8.0;
                platform.low = find_lowest_floor_surrounding(sec.clone());

                if platform.low > sec.floorheight {
                    platform.low = sec.floorheight;
                }

                platform.high = sec.floorheight;
                platform.wait = TICRATE * PLATWAIT;
                platform.status = PlatStatus::Down;
                // TODO: S_StartSound(&sec->soundorg, sfx_pstart);
            }
        }

        let thinker = MapObject::create_thinker(ObjectType::Platform(platform), Platform::think);

        if let Some(ptr) = level.thinkers.push::<Platform>(thinker) {
            ptr.set_obj_thinker_ptr();
            sec.specialdata = Some(ptr);
        }
    }

    ret
}

impl Think for Platform {
    fn think(object: &mut ObjectType, level: &mut Level) -> bool {
        let platform = object.platform();

        match platform.status {
            PlatStatus::Up => {
                let res = move_plane(
                    platform.sector.clone(),
                    platform.speed,
                    platform.high,
                    platform.crush,
                    0,
                    1,
                );

                if matches!(
                    platform.kind,
                    PlatKind::RaiseAndChange | PlatKind::RaiseToNearestAndChange
                ) && level.level_time & 7 == 0
                {
                    // TODO: if (!(leveltime&7))
                    //  S_StartSound(&plat->sector->soundorg, sfx_stnmov);
                }

                if matches!(res, PlaneResult::Crushed) && !platform.crush {
                    platform.count = platform.wait;
                    platform.status = PlatStatus::Waiting;
                    // TODO: S_StartSound(&plat->sector->soundorg, sfx_pstart);
                } else if matches!(res, PlaneResult::PastDest) {
                    platform.count = platform.wait;
                    platform.status = PlatStatus::Waiting;
                    // TODO: S_StartSound(&plat->sector->soundorg, sfx_pstop);

                    match platform.kind {
                        PlatKind::BlazeDWUS | PlatKind::DownWaitUpStay => {
                            unsafe {
                                platform.sector.specialdata = None; // TODO: remove when tracking active?
                                (*platform.thinker).mark_remove();
                            }
                            // TODO: P_RemoveActivePlat(plat);
                        }
                        PlatKind::RaiseAndChange | PlatKind::RaiseToNearestAndChange => {
                            unsafe {
                                platform.sector.specialdata = None; // TODO: remove when tracking active?
                                (*platform.thinker).mark_remove();
                            }
                            // TODO: P_RemoveActivePlat(plat);
                        }
                        _ => {}
                    }
                }
            }
            PlatStatus::Down => {
                let res = move_plane(
                    platform.sector.clone(),
                    platform.speed,
                    platform.low,
                    false,
                    0,
                    -1,
                );

                if matches!(res, PlaneResult::PastDest) {
                    platform.count = platform.wait;
                    platform.status = PlatStatus::Waiting;
                    // TODO: S_StartSound(&plat->sector->soundorg, sfx_pstop);
                }
            }
            PlatStatus::Waiting => {
                platform.count -= 1;
                if platform.count == 0 {
                    if platform.sector.floorheight == platform.low {
                        platform.status = PlatStatus::Up;
                    } else {
                        platform.status = PlatStatus::Down;
                    }
                    // TODO: S_StartSound(&plat->sector->soundorg, sfx_pstart);
                }
            }
            PlatStatus::InStasis => {}
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
