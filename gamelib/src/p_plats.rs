use std::ptr::NonNull;

use crate::{
    d_thinker::{ActionF, Think, Thinker, ThinkerType},
    doom_def::TICRATE,
    level_data::{level::Level, map_defs::LineDef},
    p_floor::move_plane,
    p_map_object::MapObject,
    p_spec::{
        find_highest_floor_surrounding, find_lowest_floor_surrounding, PlatKind, PlatStatus,
        Platform, ResultE,
    },
    DPtr,
};

// TODO: active platform tracking? Seems to be required for "animated" platforms.

const PLATSPEED: f32 = 1.0;
const PLATWAIT: i32 = 3;

pub fn ev_do_platform(line: DPtr<LineDef>, kind: PlatKind, amount: i32, level: &mut Level) -> bool {
    let mut ret = false;

    if matches!(kind, PlatKind::perpetualRaise) {
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
            thinker: NonNull::dangling(),
            sector: DPtr::new(sector),
            speed: PLATSPEED,
            low: 0.0,
            high: 0.0,
            wait: 0,
            count: 0,
            status: PlatStatus::in_stasis,
            old_status: PlatStatus::in_stasis,
            crush: false,
            tag: line.tag,
            kind,
        };

        match kind {
            PlatKind::raiseToNearestAndChange => {
                platform.speed /= 2.0;
                platform.high = find_highest_floor_surrounding(sec.clone());
                platform.wait = 0;
                platform.status = PlatStatus::up;
                sec.special = 0;
                // TODO: sec->floorpic = sides[line->sidenum[0]].sector->floorpic;
                // TODO: S_StartSound(&sec->soundorg, sfx_stnmov);
            }
            PlatKind::raiseAndChange => {
                platform.speed /= 2.0;
                platform.high = sec.floorheight + amount as f32;
                platform.wait = 0;
                platform.status = PlatStatus::up;
                // TODO: sec->floorpic = sides[line->sidenum[0]].sector->floorpic;
                // TODO: S_StartSound(&sec->soundorg, sfx_stnmov);
            }

            PlatKind::perpetualRaise => {
                platform.low = find_lowest_floor_surrounding(sec.clone());

                if platform.low > sec.floorheight {
                    platform.low = sec.floorheight;
                }

                platform.high = find_highest_floor_surrounding(sec.clone());

                if platform.high < sec.floorheight {
                    platform.high = sec.floorheight;
                }

                platform.wait = TICRATE * PLATWAIT;

                platform.status = PlatStatus::down;
                // TODO: plat->status = P_Random() & 1;
                // TODO: S_StartSound(&sec->soundorg, sfx_pstart);
            }
            PlatKind::downWaitUpStay => {
                platform.speed *= 4.0;
                platform.low = find_lowest_floor_surrounding(sec.clone());

                if platform.low > sec.floorheight {
                    platform.low = sec.floorheight;
                }

                platform.high = sec.floorheight;
                platform.wait = TICRATE * PLATWAIT;
                platform.status = PlatStatus::down;
                // TODO: S_StartSound(&sec->soundorg, sfx_pstart);
            }
            PlatKind::blazeDWUS => {
                platform.speed *= 8.0;
                platform.low = find_lowest_floor_surrounding(sec.clone());

                if platform.low > sec.floorheight {
                    platform.low = sec.floorheight;
                }

                platform.high = sec.floorheight;
                platform.wait = TICRATE * PLATWAIT;
                platform.status = PlatStatus::down;
                // TODO: S_StartSound(&sec->soundorg, sfx_pstart);
            }
        }

        let thinker = MapObject::create_thinker(
            ThinkerType::Platform(platform),
            ActionF::Action1(Platform::think),
        );

        if let Some(mut ptr) = level.add_thinker::<Platform>(thinker) {
            unsafe {
                ptr.as_mut()
                    .obj_mut()
                    .bad_mut::<Platform>()
                    .set_thinker_ptr(ptr);

                sec.specialdata = Some(ptr);
            }
        }
    }

    ret
}

impl Think for Platform {
    fn think(
        object: &mut crate::d_thinker::ThinkerType,
        level: &mut crate::level_data::level::Level,
    ) -> bool {
        let platform = object.bad_mut::<Platform>();
        match platform.status {
            PlatStatus::up => {
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
                    PlatKind::raiseAndChange | PlatKind::raiseToNearestAndChange
                ) && level.level_time & 7 == 0
                {
                    // TODO: if (!(leveltime&7))
                    //  S_StartSound(&plat->sector->soundorg, sfx_stnmov);
                }

                if matches!(res, ResultE::Crushed) && !platform.crush {
                    platform.count = platform.wait;
                    platform.status = PlatStatus::waiting;
                    // TODO: S_StartSound(&plat->sector->soundorg, sfx_pstart);
                } else if matches!(res, ResultE::PastDest) {
                    platform.count = platform.wait;
                    platform.status = PlatStatus::waiting;
                    // TODO: S_StartSound(&plat->sector->soundorg, sfx_pstop);

                    match platform.kind {
                        PlatKind::blazeDWUS | PlatKind::downWaitUpStay => {
                            unsafe {
                                platform.thinker.as_mut().set_action(ActionF::Remove);
                                platform.sector.specialdata = None; // TODO: remove when tracking active?
                            }
                            // TODO: P_RemoveActivePlat(plat);
                        }
                        PlatKind::raiseAndChange | PlatKind::raiseToNearestAndChange => {
                            unsafe {
                                platform.thinker.as_mut().set_action(ActionF::Remove);
                                platform.sector.specialdata = None; // TODO: remove when tracking active?
                            }
                            // TODO: P_RemoveActivePlat(plat);
                        }
                        _ => {}
                    }
                }
            }
            PlatStatus::down => {
                let res = move_plane(
                    platform.sector.clone(),
                    platform.speed,
                    platform.low,
                    false,
                    0,
                    -1,
                );

                if matches!(res, ResultE::PastDest) {
                    platform.count = platform.wait;
                    platform.status = PlatStatus::waiting;
                    // TODO: S_StartSound(&plat->sector->soundorg, sfx_pstop);
                }
            }
            PlatStatus::waiting => {
                platform.count -= 1;
                if platform.count == 0 {
                    if platform.sector.floorheight == platform.low {
                        platform.status = PlatStatus::up;
                    } else {
                        platform.status = PlatStatus::down;
                    }
                    // TODO: S_StartSound(&plat->sector->soundorg, sfx_pstart);
                }
            }
            PlatStatus::in_stasis => {}
        }

        true
    }

    fn set_thinker_ptr(&mut self, ptr: std::ptr::NonNull<Thinker>) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> NonNull<Thinker> {
        self.thinker
    }
}
