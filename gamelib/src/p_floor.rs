//! Floor movement thinker: raise, lower, crusher
use std::ptr::NonNull;

use crate::{
    d_thinker::{ActionF, Think, Thinker, ThinkerType},
    level_data::{
        level::Level,
        map_defs::{LineDef, Sector},
    },
    p_map_object::MapObject,
    p_specials::{
        find_highest_floor_surrounding, find_lowest_ceiling_surrounding,
        find_lowest_floor_surrounding, find_next_highest_floor, move_plane, PlaneResult,
    },
    DPtr,
};

const FLOORSPEED: f32 = 1.0;
const ML_TWOSIDED: i16 = 4;

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
pub enum StairEnum {
    /// slowly build by 8
    Build8,
    /// quickly build by 16
    Turbo16,
}

pub struct FloorMove {
    pub thinker: NonNull<Thinker>,
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
                let mut min = sec.floorheight;
                floor.direction = 1;
                for line in sec.lines.iter() {
                    if line.flags & ML_TWOSIDED != 0 {
                        todo!("side = getSide(secnum, i, 0); and stuff");
                    }
                }
                floor.destheight = sec.floorheight + min;
            }
            FloorKind::LowerAndChange => todo!(),
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

        if matches!(res, PlaneResult::PastDest) {
            if floor.direction == 1 && matches!(floor.kind, FloorKind::DonutRaise)
                || floor.direction == -1 && matches!(floor.kind, FloorKind::LowerAndChange)
            {
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
