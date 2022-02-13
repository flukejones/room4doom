// T_MovePlane

use std::ptr::NonNull;

use log::{debug, error};

use crate::{
    d_thinker::{ActionF, Think, Thinker, ThinkerType},
    level_data::map_defs::Sector,
    p_map::change_sector,
    p_spec::{FloorKind, FloorMove, ResultE},
    DPtr,
};

pub fn move_plane(
    mut sector: DPtr<Sector>,
    speed: f32,
    dest: f32,
    crush: bool,
    floor_or_ceiling: i32,
    direction: i32,
) -> ResultE {
    match floor_or_ceiling {
        0 => {
            // FLOOR
            match direction {
                -1 => {
                    // DOWN
                    debug!(
                        "move_plane: floor: down: {} to {} at speed {}",
                        sector.floorheight, dest, speed
                    );
                    if sector.floorheight - speed < dest {
                        let last_pos = sector.floorheight;
                        sector.floorheight = dest;

                        if change_sector(sector.clone(), crush) {
                            sector.floorheight = last_pos;
                            change_sector(sector, crush);
                        }
                        return ResultE::PastDest;
                    } else {
                        // COULD GET CRUSHED
                        let last_pos = sector.floorheight;
                        sector.floorheight -= speed;

                        if change_sector(sector.clone(), crush) {
                            if crush {
                                return ResultE::Crushed;
                            }
                            sector.floorheight = last_pos;
                            change_sector(sector, crush);
                            return ResultE::Crushed;
                        }
                    }
                }
                1 => {
                    // UP
                    debug!(
                        "move_plane: floor: up: {} to {} at speed {}",
                        sector.floorheight, dest, speed
                    );
                    if sector.floorheight + speed > dest {
                        let last_pos = sector.floorheight;
                        sector.floorheight = dest;

                        if change_sector(sector.clone(), crush) {
                            sector.floorheight = last_pos;
                            change_sector(sector, crush);
                        }
                        return ResultE::PastDest;
                    } else {
                        let last_pos = sector.floorheight;
                        sector.floorheight += speed;
                        if change_sector(sector.clone(), crush) {
                            if crush {
                                return ResultE::Crushed;
                            }
                            sector.floorheight = last_pos;
                            change_sector(sector, crush);
                            return ResultE::Crushed;
                        }
                    }
                }
                _ => error!("Invalid floor direction: {}", direction),
            }
        }
        1 => {
            // CEILING
            match direction {
                -1 => {
                    // DOWN
                    debug!(
                        "move_plane: ceiling: down: {} to {} at speed {}",
                        sector.ceilingheight, dest, speed
                    );
                    if sector.ceilingheight - speed < dest {
                        let last_pos = sector.ceilingheight;
                        sector.ceilingheight = dest;

                        if change_sector(sector.clone(), crush) {
                            sector.ceilingheight = last_pos;
                            change_sector(sector, crush);
                        }
                        return ResultE::PastDest;
                    } else {
                        // COULD GET CRUSHED
                        let last_pos = sector.ceilingheight;
                        sector.ceilingheight -= speed;

                        if change_sector(sector.clone(), crush) {
                            if crush {
                                return ResultE::Crushed;
                            }
                            sector.ceilingheight = last_pos;
                            change_sector(sector, crush);
                            return ResultE::Crushed;
                        }
                    }
                }
                1 => {
                    // UP
                    debug!(
                        "move_plane: ceiling: up: {} to {} at speed {}",
                        sector.ceilingheight, dest, speed
                    );
                    if sector.ceilingheight + speed > dest {
                        let last_pos = sector.ceilingheight;
                        sector.ceilingheight = dest;

                        if change_sector(sector.clone(), crush) {
                            sector.ceilingheight = last_pos;
                            change_sector(sector, crush);
                        }
                        return ResultE::PastDest;
                    } else {
                        //let last_pos = sector.ceilingheight;
                        sector.ceilingheight += speed;
                        change_sector(sector, crush);
                    }
                }
                _ => error!("Invalid ceiling direction: {}", direction),
            }
        }
        _ => error!("Invalid floor_or_ceiling: {}", floor_or_ceiling),
    }

    ResultE::Ok
}

impl Think for FloorMove {
    fn think(object: &mut ThinkerType, level: &mut crate::level_data::level::Level) -> bool {
        let floor = object.bad_mut::<FloorMove>();
        let res = move_plane(
            floor.sector.clone(),
            floor.speed,
            floor.floordestheight,
            false,
            0,
            floor.direction,
        );

        if level.level_time & 7 == 0 {
            // TODO: if (!(leveltime & 7))
            //  S_StartSound(&floor->sector->soundorg, sfx_stnmov);
        }

        if matches!(res, ResultE::PastDest) {
            floor.sector.specialdata = None;

            if floor.direction == 1 && matches!(floor.kind, FloorKind::donutRaise) {
                floor.sector.special = floor.newspecial;
                //TODO: floor.sector.floorpic = floor.texture;
            } else if floor.direction == -1 && matches!(floor.kind, FloorKind::lowerAndChange) {
                floor.sector.special = floor.newspecial;
                //TODO: floor.sector.floorpic = floor.texture;
            }

            floor.thinker_mut().set_action(ActionF::None);
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
