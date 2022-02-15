// T_MovePlane

use std::ptr::NonNull;

use crate::{
    d_thinker::{ActionF, Think, Thinker, ThinkerType},
    level_data::{level::Level, map_defs::LineDef},
    p_floor::move_plane,
    p_map_object::MapObject,
    p_spec::{find_highest_ceiling_surrounding, CeilingKind, CeilingMove, ResultE},
    DPtr,
};

const CEILSPEED: f32 = 1.0;

// TODO: track activeceilings

/// EV_DoFloor
pub fn ev_do_ceiling(line: DPtr<LineDef>, kind: CeilingKind, level: &mut Level) -> bool {
    let mut ret = false;

    if matches!(
        kind,
        CeilingKind::fastCrushAndRaise
            | CeilingKind::silentCrushAndRaise
            | CeilingKind::crushAndRaise
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
            thinker: NonNull::dangling(),
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
            CeilingKind::lowerToFloor => {
                ceiling.crush = true;
                ceiling.topheight = sec.ceilingheight;
                ceiling.bottomheight = sec.floorheight;
                ceiling.direction = -1;
            }
            CeilingKind::raiseToHighest => {
                ceiling.topheight = find_highest_ceiling_surrounding(sec.clone());
                ceiling.bottomheight = sec.floorheight;
                ceiling.direction = 1;
            }
            CeilingKind::lowerAndCrush => {
                ceiling.crush = true;
                ceiling.topheight = sec.ceilingheight;
                ceiling.bottomheight = sec.floorheight + 8.0;
                ceiling.direction = -1;
            }
            CeilingKind::crushAndRaise | CeilingKind::silentCrushAndRaise => {
                ceiling.crush = true;
                ceiling.topheight = sec.ceilingheight;
                ceiling.bottomheight = sec.floorheight + 8.0;
                ceiling.direction = -1;
            }
            CeilingKind::fastCrushAndRaise => {
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
            ActionF::Action1(CeilingMove::think),
        );

        if let Some(mut ptr) = level.add_thinker::<CeilingMove>(thinker) {
            unsafe {
                ptr.as_mut()
                    .obj_mut()
                    .bad_mut::<CeilingMove>()
                    .set_thinker_ptr(ptr);

                sec.specialdata = Some(ptr);
            }
        }
    }

    ret
}

impl Think for CeilingMove {
    fn think(object: &mut ThinkerType, level: &mut crate::level_data::level::Level) -> bool {
        let ceiling = object.bad_mut::<CeilingMove>();

        if level.level_time & 7 == 0 && !matches!(ceiling.kind, CeilingKind::silentCrushAndRaise) {
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

                if matches!(res, ResultE::PastDest) {
                    match ceiling.kind {
                        CeilingKind::raiseToHighest => unsafe {
                            ceiling.sector.specialdata = None;
                            ceiling.thinker.as_mut().set_action(ActionF::Remove);
                        },
                        CeilingKind::crushAndRaise | CeilingKind::fastCrushAndRaise => {
                            ceiling.direction = -1;
                        }
                        CeilingKind::silentCrushAndRaise => {
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

                if matches!(res, ResultE::PastDest) {
                    match ceiling.kind {
                        CeilingKind::lowerToFloor | CeilingKind::lowerAndCrush => unsafe {
                            ceiling.sector.specialdata = None;
                            ceiling.thinker.as_mut().set_action(ActionF::Remove);
                        },
                        CeilingKind::crushAndRaise => {
                            ceiling.speed = CEILSPEED;
                            ceiling.direction = -1;
                        }
                        CeilingKind::fastCrushAndRaise => {
                            ceiling.speed = CEILSPEED;
                            ceiling.direction = -1;
                        }
                        CeilingKind::silentCrushAndRaise => {
                            ceiling.speed = CEILSPEED;
                            ceiling.direction = -1;
                            //TODO: S_StartSound(&ceiling->sector->soundorg, sfx_pstop);
                        }
                        _ => {}
                    }
                } else if matches!(res, ResultE::Crushed) {
                    match ceiling.kind {
                        CeilingKind::silentCrushAndRaise
                        | CeilingKind::crushAndRaise
                        | CeilingKind::lowerAndCrush => {
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

    fn set_thinker_ptr(&mut self, ptr: std::ptr::NonNull<Thinker>) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> NonNull<Thinker> {
        self.thinker
    }
}
