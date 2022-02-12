// T_MovePlane

use log::{debug, error};

use crate::{level_data::map_defs::Sector, p_map::change_sector, p_spec::ResultE, DPtr};

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
