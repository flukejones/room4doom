// T_MovePlane

use std::ptr::NonNull;

use log::{debug, error};

use crate::{
    d_thinker::{ActionF, Think, Thinker, ThinkerType},
    level_data::{
        level::Level,
        map_defs::{LineDef, Sector},
    },
    p_map::change_sector,
    p_map_object::MapObject,
    p_spec::{
        find_highest_floor_surrounding, find_lowest_ceiling_surrounding,
        find_lowest_floor_surrounding, find_next_highest_floor, FloorKind, FloorMove, ResultE,
    },
    DPtr,
};

const FLOORSPEED: f32 = 1.0;
const ML_TWOSIDED: i16 = 4;

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

/// EV_DoFloor
pub fn ev_do_floor(line: DPtr<LineDef>, kind: FloorKind, level: &mut Level) -> bool {
    let mut ret = false;

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

        let mut floor = FloorMove {
            thinker: NonNull::dangling(),
            sector: DPtr::new(sector),
            kind,
            speed: FLOORSPEED,
            crush: false,
            direction: 0,
            newspecial: 0,
            texture: 0,
            destheight: 0.0,
        };

        match kind {
            FloorKind::lowerFloor => {
                floor.direction = -1;
                floor.destheight = find_highest_floor_surrounding(sec.clone());
            }
            FloorKind::lowerFloorToLowest => {
                floor.direction = -1;
                floor.destheight = find_lowest_floor_surrounding(sec.clone());
            }
            FloorKind::turboLower => {
                floor.direction = -1;
                floor.speed *= 4.0;
                floor.destheight = find_highest_floor_surrounding(sec.clone());
                // TODO: if (gameversion <= exe_doom_1_2 ||
                //  floor->floordestheight != sec->floorheight)
                //  floor->floordestheight += 8 * FRACUNIT;
                if floor.destheight != sec.floorheight {
                    floor.destheight += 8.0;
                }
            }
            FloorKind::raiseFloor => {
                floor.direction = 1;
                floor.destheight = find_lowest_ceiling_surrounding(sec.clone());
                if floor.destheight > sec.ceilingheight {
                    floor.destheight = sec.ceilingheight;
                }
                if matches!(kind, FloorKind::raiseFloorCrush) {
                    floor.destheight -= 8.0;
                }
            }
            FloorKind::raiseFloorToNearest => {
                floor.direction = 1;
                floor.destheight = find_next_highest_floor(sec.clone(), sec.floorheight);
            }
            FloorKind::raiseToTexture => {
                // TODO: int minsize = INT_MAX;
                let mut min = sec.floorheight;
                floor.direction = 1;
                for line in sec.lines.iter() {
                    if line.flags & ML_TWOSIDED != 0 {
                        todo!("side = getSide(secnum, i, 0); and stuff");
                    }
                }
                floor.destheight = sec.floorheight + min;
            }
            FloorKind::lowerAndChange => todo!(),
            FloorKind::raiseFloor24 => {
                floor.direction = 1;
                floor.destheight = sec.floorheight + 24.0;
            }
            FloorKind::raiseFloor24AndChange => {
                floor.direction = 1;
                floor.destheight = sec.floorheight + 24.0;
                sec.floorpic = line.frontsector.floorpic;
                sec.special = line.frontsector.special;
            }
            FloorKind::raiseFloorCrush => floor.crush = true,
            FloorKind::raiseFloorTurbo => {
                floor.direction = 1;
                floor.speed *= 4.0;
                floor.destheight = find_next_highest_floor(sec.clone(), sec.floorheight);
            }
            FloorKind::donutRaise => todo!(),
            FloorKind::raiseFloor512 => {
                floor.direction = 1;
                floor.destheight = sec.floorheight + 512.0;
            }
        }

        ret = true;

        let thinker = MapObject::create_thinker(
            ThinkerType::FloorMove(floor),
            ActionF::Action1(FloorMove::think),
        );

        if let Some(mut ptr) = level.thinkers.push::<FloorMove>(thinker) {
            unsafe {
                ptr.as_mut()
                    .obj_mut()
                    .bad_mut::<FloorMove>()
                    .set_thinker_ptr(ptr);

                sec.specialdata = Some(ptr);
            }
        }
    }

    ret
}

impl Think for FloorMove {
    fn think(object: &mut ThinkerType, level: &mut Level) -> bool {
        let floor = object.bad_mut::<FloorMove>();
        let res = move_plane(
            floor.sector.clone(),
            floor.speed,
            floor.destheight,
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

            floor.sector.specialdata = None;
            floor.thinker_mut().set_action(ActionF::Remove);
        }

        true
    }

    fn set_thinker_ptr(&mut self, ptr: NonNull<Thinker>) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> NonNull<Thinker> {
        self.thinker
    }
}
