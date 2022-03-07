//! Floor movement thinker: raise, lower, crusher
//!
//! Doom source name `p_floor`
use std::ptr::null_mut;

use crate::{
    level_data::{
        map_defs::{LineDef, Sector},
        Level,
    },
    DPtr, LineDefFlags,
};

use super::{
    d_thinker::{ObjectType, Think, Thinker},
    map_object::MapObject,
    specials::{
        find_highest_floor_surrounding, find_lowest_ceiling_surrounding,
        find_lowest_floor_surrounding, find_next_highest_floor, move_plane, PlaneResult,
    },
};

const FLOORSPEED: f32 = 1.0;

#[derive(Debug, Clone, Copy)]
pub enum FloorKind {
    /// lower floor to highest surrounding floor
    LowerFloor,
    /// lower floor to lowest surrounding floor
    LowerFloorToLowest,
    /// lower floor to highest surrounding floor VERY FAST
    TurboLower,
    /// raise floor to lowest surrounding CEILING
    RaiseFloor,
    /// raise floor to next highest surrounding floor
    RaiseFloorToNearest,
    /// raise floor to shortest height with same texture around it
    RaiseToTexture,
    /// lower floor to lowest surrounding floor and change floorpic
    LowerAndChange,
    /// Raise floor 24 units from start
    RaiseFloor24,
    /// Raise floor 24 units from start and change texture
    RaiseFloor24andChange,
    /// Raise floor and crush all entities on it
    RaiseFloorCrush,
    /// raise to next highest floor, turbo-speed
    RaiseFloorTurbo,
    /// Do donuts
    DonutRaise,
    /// Raise floor 512 units from start
    RaiseFloor512,
}

/// Very special kind of thinker used specifically for building a set of stairs
/// that raises one-by-one.
#[derive(Debug, Clone, Copy)]
pub enum StairKind {
    /// slowly build by 8
    Build8,
    /// quickly build by 16
    Turbo16,
}

pub struct FloorMove {
    pub thinker: *mut Thinker,
    pub sector: DPtr<Sector>,
    pub kind: FloorKind,
    pub speed: f32,
    pub crush: bool,
    pub direction: i32,
    pub newspecial: i16,
    pub texture: u8,
    pub destheight: f32,
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
            thinker: null_mut(),
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
            FloorKind::LowerFloor => {
                floor.direction = -1;
                floor.destheight = find_highest_floor_surrounding(sec.clone());
            }
            FloorKind::LowerFloorToLowest => {
                floor.direction = -1;
                floor.destheight = find_lowest_floor_surrounding(sec.clone());
            }
            FloorKind::TurboLower => {
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
            FloorKind::RaiseFloor => {
                floor.direction = 1;
                floor.destheight = find_lowest_ceiling_surrounding(sec.clone());
                if floor.destheight > sec.ceilingheight {
                    floor.destheight = sec.ceilingheight;
                }
                if matches!(kind, FloorKind::RaiseFloorCrush) {
                    floor.destheight -= 8.0;
                }
            }
            FloorKind::RaiseFloorToNearest => {
                floor.direction = 1;
                floor.destheight = find_next_highest_floor(sec.clone(), sec.floorheight);
            }
            FloorKind::RaiseToTexture => {
                // TODO: int minsize = INT_MAX;
                let min = sec.floorheight;
                floor.direction = 1;
                for line in sec.lines.iter() {
                    if line.flags & LineDefFlags::TwoSided as u32 != 0 {
                        todo!("side = getSide(secnum, i, 0); and stuff");
                    }
                }
                floor.destheight = sec.floorheight + min;
            }
            FloorKind::LowerAndChange => todo!("LowerAndChange"),
            FloorKind::RaiseFloor24 => {
                floor.direction = 1;
                floor.destheight = sec.floorheight + 24.0;
            }
            FloorKind::RaiseFloor24andChange => {
                floor.direction = 1;
                floor.destheight = sec.floorheight + 24.0;
                sec.floorpic = line.frontsector.floorpic;
                sec.special = line.frontsector.special;
            }
            FloorKind::RaiseFloorCrush => floor.crush = true,
            FloorKind::RaiseFloorTurbo => {
                floor.direction = 1;
                floor.speed *= 4.0;
                floor.destheight = find_next_highest_floor(sec.clone(), sec.floorheight);
            }
            FloorKind::DonutRaise => todo!(),
            FloorKind::RaiseFloor512 => {
                floor.direction = 1;
                floor.destheight = sec.floorheight + 512.0;
            }
        }

        ret = true;

        let thinker = MapObject::create_thinker(ObjectType::FloorMove(floor), FloorMove::think);

        if let Some(ptr) = level.thinkers.push::<FloorMove>(thinker) {
            unsafe {
                (*ptr).set_obj_thinker_ptr::<FloorMove>(ptr);
                sec.specialdata = Some(ptr);
            }
        }
    }

    ret
}

impl Think for FloorMove {
    fn think(object: &mut ObjectType, level: &mut Level) -> bool {
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

        if matches!(res, PlaneResult::PastDest) {
            if floor.direction == 1 && matches!(floor.kind, FloorKind::DonutRaise)
                || floor.direction == -1 && matches!(floor.kind, FloorKind::LowerAndChange)
            {
                floor.sector.special = floor.newspecial;
                //TODO: floor.sector.floorpic = floor.texture;
            }

            floor.sector.specialdata = None;
            floor.thinker_mut().mark_remove();
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

pub fn ev_build_stairs(line: DPtr<LineDef>, kind: StairKind, level: &mut Level) -> bool {
    let mut ret = false;
    let mut speed;
    let mut height;
    let mut stair_size;

    for sector in level
        .map_data
        .sectors()
        .iter()
        .filter(|s| s.tag == line.tag)
    {
        if sector.specialdata.is_some() {
            continue;
        }
        ret = true;

        let mut floor = FloorMove {
            thinker: null_mut(),
            sector: DPtr::new(sector),
            kind: FloorKind::LowerFloor,
            speed: FLOORSPEED,
            crush: false,
            direction: 1,
            newspecial: 0,
            texture: sector.floorpic as u8,
            destheight: 0.0,
        };

        match kind {
            StairKind::Build8 => {
                speed = FLOORSPEED / 4.0;
                stair_size = 8.0;
            }
            StairKind::Turbo16 => {
                speed = FLOORSPEED * 8.0;
                stair_size = 16.0;
            }
        }
        floor.speed = speed;
        height = sector.floorheight + stair_size;
        floor.destheight = height;

        // Because we need to break lifetimes...
        let mut sec = DPtr::new(sector);

        let thinker = MapObject::create_thinker(ObjectType::FloorMove(floor), FloorMove::think);

        if let Some(ptr) = level.thinkers.push::<FloorMove>(thinker) {
            unsafe {
                (*ptr).set_obj_thinker_ptr::<FloorMove>(ptr);
                sec.specialdata = Some(ptr);
            }
        }

        let texture = sec.floorpic;

        loop {
            let mut ok = false;

            for line in level
                .map_data
                .linedefs()
                .iter()
                .filter(|s| s.flags & LineDefFlags::TwoSided as u32 != 0)
            {
                // Lines need to be in the same sector, can check this with the pointer
                let mut tsec = line.frontsector.clone();

                if tsec != sec {
                    continue;
                }
                tsec = line.backsector.as_ref().unwrap().clone();

                if tsec.floorpic != texture {
                    continue;
                }

                height += stair_size;
                if tsec.specialdata.is_some() {
                    continue;
                }
                sec = tsec;

                // New thinker
                let floor = FloorMove {
                    thinker: null_mut(),
                    sector: sec.clone(),
                    kind: FloorKind::LowerFloor,
                    speed,
                    crush: false,
                    direction: 1,
                    newspecial: 0,
                    texture: sector.floorpic as u8,
                    destheight: height,
                };

                let thinker =
                    MapObject::create_thinker(ObjectType::FloorMove(floor), FloorMove::think);

                if let Some(ptr) = level.thinkers.push::<FloorMove>(thinker) {
                    unsafe {
                        (*ptr).set_obj_thinker_ptr::<FloorMove>(ptr);
                        sec.specialdata = Some(ptr);
                    }
                }

                ok = true;
                break;
            }

            if !ok {
                break;
            }
        }
    }

    ret
}
