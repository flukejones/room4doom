//! Vanilla mover specials expressed as [`MoveEffect`] presets.
//!
//! Doom source names `p_doors`, `p_floor`, `p_ceiling`, `p_plats`. Destination
//! heights come from [`level::env_target`] (the single source of mover target
//! math, shared with tooling previews); each preset sets only policy: speed,
//! phase, crush, arrival, change, sounds.

use std::ptr;

use log::{debug, error};
use sound_common::SfxName;

use super::{
    ArrivalAction, CrushResponse, MoveEffect, MoveTarget, MoverChange, MoverPhase, MoverPlane,
    MoverSounds, base_mover, push_mover,
};
use crate::doom_def::TICRATE;
use crate::env::switch::start_sector_sound;
use crate::level::LevelState;
use crate::thing::MapObject;
use crate::thinker::Thinker;
use level::env_query::get_next_sector;
use level::env_target::{ceiling_target, door_top, floor_target, plat_target, shortest_texture};
use level::map_defs::{LineDef, Sector, SectorHeight};
use level::special_encode::{CeilKind, DoorKind, FloorKind, PlatKind, SectorSpecial, StairKind};
use level::{LineDefFlags, MapPtr, MovementType};
use math::{FixedT, p_random};

const VDOORSPEED: SectorHeight = FixedT::from_fixed(2 << 16);
const VDOORWAIT: i32 = 150;
/// Hold at the bottom for `Close30ThenOpen`, and the fuse of a type-10 sector.
const DOOR_CLOSE_WAIT: i32 = TICRATE * 30;
/// Fuse of a type-14 sector (`P_SpawnDoorRaiseIn5Mins`).
const DOOR_RAISE_WAIT: i32 = TICRATE * 60 * 5;
const FLOORSPEED: SectorHeight = SectorHeight::ONE;
const CEILSPEED: SectorHeight = SectorHeight::ONE;
const PLATSPEED: SectorHeight = SectorHeight::ONE;
const PLATWAIT: i32 = 3;

/// Sounds every non-blazing door uses.
fn door_sounds() -> MoverSounds {
    MoverSounds {
        start_up: Some(SfxName::Doropn),
        start_down: Some(SfxName::Dorcls),
        ..MoverSounds::default()
    }
}

/// Build the door for one sector. Returns the door plus the sound its opening
/// (or closing) plays, which the caller emits on the triggering line.
fn build_door(sec: MapPtr<Sector>, kind: DoorKind) -> Option<(MoveEffect, Option<SfxName>)> {
    let top = door_top(sec.clone());
    let ceiling = sec.ceilingheight;
    let mut door = base_mover(sec, MoverPlane::Ceiling, VDOORSPEED);
    door.low = MoveTarget::LiveFloor;
    door.high = top;
    door.wait = VDOORWAIT;
    door.sounds = door_sounds();
    // Close-only doors override this: they keep pushing rather than bouncing.
    door.crush_response = CrushResponse::Reverse;

    let sfx = match kind {
        DoorKind::Normal => {
            door.phase = MoverPhase::Up;
            door.at_top = ArrivalAction::WaitThenReverse;
            (top != ceiling).then_some(SfxName::Doropn)
        }
        DoorKind::Open => {
            door.phase = MoverPhase::Up;
            (top != ceiling).then_some(SfxName::Doropn)
        }
        DoorKind::BlazeRaise => {
            door.speed = VDOORSPEED * 4;
            door.base_speed = door.speed;
            door.phase = MoverPhase::Up;
            door.at_top = ArrivalAction::WaitThenReverse;
            door.sounds.start_down = Some(SfxName::Bdcls);
            door.sounds.finish = Some(SfxName::Bdcls);
            (top != ceiling).then_some(SfxName::Bdopn)
        }
        DoorKind::BlazeOpen => {
            door.speed = VDOORSPEED * 4;
            door.base_speed = door.speed;
            door.phase = MoverPhase::Up;
            (top != ceiling).then_some(SfxName::Bdopn)
        }
        DoorKind::BlazeClose => {
            door.speed = VDOORSPEED * 4;
            door.base_speed = door.speed;
            door.phase = MoverPhase::Down;
            door.crush_response = CrushResponse::Ignore;
            door.sounds.finish = Some(SfxName::Bdcls);
            Some(SfxName::Bdcls)
        }
        DoorKind::Close => {
            door.phase = MoverPhase::Down;
            door.crush_response = CrushResponse::Ignore;
            Some(SfxName::Dorcls)
        }
        DoorKind::Close30ThenOpen => {
            door.high = ceiling;
            door.phase = MoverPhase::Down;
            door.wait = DOOR_CLOSE_WAIT;
            door.at_bottom = ArrivalAction::WaitThenReverse;
            Some(SfxName::Dorcls)
        }
        DoorKind::RaiseIn5Mins => {
            // Spawned by sector type 14, never by a line special.
            error!("RaiseIn5Mins cannot be line-triggered");
            return None;
        }
    };

    Some((door, sfx))
}

/// EV_DoDoor. Can affect multiple sectors via the sector tag.
pub fn ev_do_door(line: MapPtr<LineDef>, kind: DoorKind, level: &mut LevelState) -> bool {
    let mut ret = false;
    for sector in level
        .level_data
        .sectors
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        if sector.specialdata.is_some() {
            continue;
        }
        let Some((door, sfx)) = build_door(MapPtr::new(sector), kind) else {
            continue;
        };

        ret = true;
        if let Some(sfx) = sfx {
            start_sector_sound(&line, sfx, &level.snd_command);
        }
        push_mover(door, &mut level.thinkers);
    }

    ret
}

/// EV_VerticalDoor. Manual (push-activated) doors; the key check happens at
/// dispatch before this is called.
pub fn ev_vertical_door(
    mut line: MapPtr<LineDef>,
    thing: &MapObject,
    kind: DoorKind,
    repeatable: bool,
    level: &mut LevelState,
) {
    if !line.flags.contains(LineDefFlags::TwoSided) {
        error!("ev_vertical_door: tried to operate on a line that is not two-sided");
        return;
    }

    let sec = line.backsector.clone().unwrap();

    // An already-moving raise door reverses instead of spawning a second one;
    // open/close kinds fall through and spawn a second thinker, as vanilla does.
    if let Some(data) = sec.specialdata
        && matches!(kind, DoorKind::Normal | DoorKind::BlazeRaise)
    {
        // SAFETY: specialdata is only ever set to a live mover thinker in this arena.
        let mover = unsafe { &mut *data.cast::<Thinker>() }.mover_mut();
        if matches!(mover.phase, MoverPhase::Down) {
            mover.phase = MoverPhase::Up;
        } else {
            if thing.player().is_none() {
                return; // bad guys never close doors
            }
            mover.phase = MoverPhase::Down;
        }
        return;
    }

    if !repeatable {
        line.special = None;
    }
    let sfx = match kind {
        DoorKind::BlazeRaise | DoorKind::BlazeOpen => SfxName::Bdopn,
        _ => SfxName::Doropn,
    };

    let Some((door, _)) = build_door(sec, kind) else {
        return;
    };
    debug!("Activated manual door in sector {}", door.sector.num);
    start_sector_sound(&line, sfx, &level.snd_command);
    push_mover(door, &mut level.thinkers);
}

/// Sector type 10: door that closes 30 seconds after level start.
/// Doom function name `P_SpawnDoorCloseIn30`
pub fn spawn_door_close_in_30(sector: &mut Sector, level: &mut LevelState) {
    sector.special = SectorSpecial::default();
    let mut door = base_mover(MapPtr::new(sector), MoverPlane::Ceiling, VDOORSPEED);
    door.low = MoveTarget::LiveFloor;
    door.high = door.sector.ceilingheight;
    door.sounds = door_sounds();
    door.phase = MoverPhase::Wait;
    door.countdown = DOOR_CLOSE_WAIT;
    push_mover(door, &mut level.thinkers);
}

/// Sector type 14: door that raises 5 minutes after level start, then behaves
/// as a normal door. Doom function name `P_SpawnDoorRaiseIn5Mins`
pub fn spawn_door_raise_in_5_mins(sector: &mut Sector, level: &mut LevelState) {
    sector.special = SectorSpecial::default();
    let sec = MapPtr::new(sector);
    let mut door = base_mover(sec.clone(), MoverPlane::Ceiling, VDOORSPEED);
    door.low = MoveTarget::LiveFloor;
    door.high = door_top(sec);
    door.wait = VDOORWAIT;
    door.at_top = ArrivalAction::WaitThenReverse;
    door.sounds = door_sounds();
    door.crush_response = CrushResponse::Reverse;
    door.phase = MoverPhase::Wait;
    door.countdown = DOOR_RAISE_WAIT;
    push_mover(door, &mut level.thinkers);
}

/// OG floor movers travel a fixed direction per kind, independent of where the
/// computed destination lies.
fn floor_up(kind: FloorKind) -> bool {
    matches!(
        kind,
        FloorKind::RaiseFloor
            | FloorKind::RaiseFloorToNearest
            | FloorKind::RaiseToTexture
            | FloorKind::RaiseFloor24
            | FloorKind::RaiseFloor24andChange
            | FloorKind::RaiseFloorCrush
            | FloorKind::RaiseFloorTurbo
            | FloorKind::RaiseFloor512
    )
}

/// Floor texture and special picked up from the neighbour the floor lands on.
fn landing_neighbour_change(sec: &MapPtr<Sector>, dest: SectorHeight) -> MoverChange {
    let mut change = MoverChange {
        newspecial: SectorSpecial::default(),
        texture: sec.floorpic,
    };
    for line in &sec.lines {
        if !line.flags.contains(LineDefFlags::TwoSided) {
            continue;
        }
        let neighbour = if line.front_sidedef.sector == *sec {
            line.back_sidedef.as_ref().unwrap().sector.clone()
        } else {
            line.front_sidedef.sector.clone()
        };
        if neighbour.floorheight == dest {
            change.texture = neighbour.floorpic;
            change.newspecial = neighbour.special;
            break;
        }
    }
    change
}

/// Point a one-shot floor at `dest`, travelling `up` or down.
fn aim_floor(floor: &mut MoveEffect, dest: SectorHeight, up: bool) {
    if up {
        floor.phase = MoverPhase::Up;
        floor.high = dest;
        floor.low = MoveTarget::Fixed(floor.sector.floorheight);
    } else {
        floor.phase = MoverPhase::Down;
        floor.low = MoveTarget::Fixed(dest);
        floor.high = floor.sector.floorheight;
    }
}

/// EV_DoFloor. `DonutRaise` routes through [`ev_do_donut`] instead.
pub fn ev_do_floor(line: MapPtr<LineDef>, kind: FloorKind, level: &mut LevelState) -> bool {
    let mut ret = false;

    for sector in level
        .level_data
        .sectors
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        if sector.specialdata.is_some() {
            continue;
        }

        let mut sec = MapPtr::new(sector);
        let mut floor = base_mover(sec.clone(), MoverPlane::Floor, FLOORSPEED);
        floor.sounds.moving = Some(SfxName::Stnmov);
        match kind {
            FloorKind::TurboLower | FloorKind::RaiseFloorTurbo => {
                floor.speed = FLOORSPEED * 4;
                floor.base_speed = floor.speed;
            }
            FloorKind::RaiseFloorCrush => floor.crush = true,
            _ => {}
        }

        let tex_min = if matches!(kind, FloorKind::RaiseToTexture) {
            shortest_texture(&sec, &|t| level.texture_heights[t])
        } else {
            SectorHeight::ZERO
        };
        let dest = floor_target(sec.clone(), kind, tex_min);
        aim_floor(&mut floor, dest, floor_up(kind));

        match kind {
            FloorKind::LowerAndChange => {
                floor.change = Some(landing_neighbour_change(&sec, dest));
            }
            FloorKind::RaiseFloor24andChange => {
                sec.floorpic = line.frontsector.floorpic;
                sec.special = line.frontsector.special;
                level
                    .level_data
                    .bsp_3d
                    .update_flat_texture(sec.num as usize, MovementType::Floor);
            }
            _ => {}
        }

        ret = true;
        push_mover(floor, &mut level.thinkers);
    }

    ret
}

/// EV_BuildStairs
pub fn ev_build_stairs(line: MapPtr<LineDef>, kind: StairKind, level: &mut LevelState) -> bool {
    let mut ret = false;
    let (speed, stair_size) = match kind {
        StairKind::Build8 => (FLOORSPEED / 4, SectorHeight::from(8)),
        StairKind::Turbo16 => (FLOORSPEED * 4, SectorHeight::from(16)),
    };

    let sector_count = level.level_data.sectors.len();
    for i in 0..sector_count {
        if level.level_data.sectors[i].tag != line.tag {
            continue;
        }

        let mut height;
        let (mut sec, texture) = {
            let sector = &mut level.level_data.sectors[i];
            if sector.specialdata.is_some() {
                continue;
            }
            ret = true;

            let sec = MapPtr::new(sector);
            height = sec.floorheight + stair_size;
            let mut step = base_mover(sec.clone(), MoverPlane::Floor, speed);
            step.sounds.moving = Some(SfxName::Stnmov);
            aim_floor(&mut step, height, true);
            push_mover(step, &mut level.thinkers);

            let texture = sec.floorpic;
            (sec, texture)
        };

        loop {
            let mut ok = false;

            for line in level
                .level_data
                .linedefs
                .iter()
                .filter(|s| s.flags.contains(LineDefFlags::TwoSided))
            {
                // Same-sector test is by pointer identity, not by sector number.
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

                let mut step = base_mover(sec.clone(), MoverPlane::Floor, speed);
                step.sounds.moving = Some(SfxName::Stnmov);
                aim_floor(&mut step, height, true);
                push_mover(step, &mut level.thinkers);

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

/// EV_DoDonut. Raises the surrounding ring to the pool floor while the pool
/// itself drops to meet it.
pub fn ev_do_donut(line: MapPtr<LineDef>, level: &mut LevelState) -> bool {
    let mut ret = false;

    for sector in level
        .level_data
        .sectors
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        if sector.specialdata.is_some() {
            continue;
        }
        ret = true;

        let Some(s2) = get_next_sector(sector.lines[0].clone(), MapPtr::new(sector)) else {
            continue;
        };

        for line in &s2.lines {
            if !line.flags.contains(LineDefFlags::TwoSided) {
                continue;
            }
            let Some(s3) = line.backsector.clone() else {
                continue;
            };
            if ptr::eq(s3.as_ref(), sector) {
                continue;
            }

            let mut ring = base_mover(s2.clone(), MoverPlane::Floor, FLOORSPEED / 2);
            ring.sounds.moving = Some(SfxName::Stnmov);
            aim_floor(&mut ring, s3.floorheight, true);
            ring.change = Some(MoverChange {
                newspecial: SectorSpecial::default(),
                texture: s3.floorpic,
            });
            push_mover(ring, &mut level.thinkers);

            let mut pool = base_mover(MapPtr::new(sector), MoverPlane::Floor, FLOORSPEED / 2);
            pool.sounds.moving = Some(SfxName::Stnmov);
            aim_floor(&mut pool, s3.floorheight, false);
            push_mover(pool, &mut level.thinkers);

            break;
        }
    }

    ret
}

/// EV_DoCeiling
pub fn ev_do_ceiling(line: MapPtr<LineDef>, kind: CeilKind, level: &mut LevelState) -> bool {
    let mut ret = false;

    if matches!(
        kind,
        CeilKind::FastCrushAndRaise | CeilKind::SilentCrushAndRaise | CeilKind::CrushAndRaise
    ) {
        // TODO: vanilla P_ActivateInStasisCeiling releases stopped crushers
        // here; ceiling stasis (specials 57/74) is not implemented.
    }

    for sector in level
        .level_data
        .sectors
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        if sector.specialdata.is_some() {
            continue;
        }

        let sec = MapPtr::new(sector);
        let mut ceiling = base_mover(sec.clone(), MoverPlane::Ceiling, CEILSPEED);
        ceiling.sounds.moving = Some(SfxName::Stnmov);

        let (top, bottom, dir) = ceiling_target(sec, kind);
        ceiling.high = top;
        ceiling.low = MoveTarget::Fixed(bottom);
        ceiling.phase = if dir > 0 {
            MoverPhase::Up
        } else {
            MoverPhase::Down
        };

        match kind {
            CeilKind::LowerToFloor | CeilKind::RaiseToHighest => {}
            // OG quirk: lowerAndCrush grinds but leaves the crush flag off.
            CeilKind::LowerAndCrush => ceiling.crush_response = CrushResponse::SlowGrind,
            CeilKind::CrushAndRaise | CeilKind::SilentCrushAndRaise => {
                ceiling.crush = true;
                ceiling.crush_response = CrushResponse::SlowGrind;
                ceiling.at_top = ArrivalAction::Reverse;
                ceiling.at_bottom = ArrivalAction::Reverse;
                if matches!(kind, CeilKind::SilentCrushAndRaise) {
                    ceiling.sounds.moving = None;
                    ceiling.sounds.start_up = Some(SfxName::Pstop);
                    ceiling.sounds.start_down = Some(SfxName::Pstop);
                }
            }
            CeilKind::FastCrushAndRaise => {
                ceiling.crush = true;
                ceiling.speed = CEILSPEED + CEILSPEED;
                ceiling.base_speed = ceiling.speed;
                ceiling.at_top = ArrivalAction::Reverse;
                ceiling.at_bottom = ArrivalAction::Reverse;
            }
        }

        ret = true;
        push_mover(ceiling, &mut level.thinkers);
    }

    ret
}

/// Sounds every lift uses.
fn plat_sounds() -> MoverSounds {
    MoverSounds {
        start_up: Some(SfxName::Pstart),
        start_down: Some(SfxName::Pstart),
        start_wait: Some(SfxName::Pstop),
        finish: Some(SfxName::Pstop),
        moving: None,
    }
}

/// EV_StopPlat
pub fn ev_stop_platform(line: MapPtr<LineDef>, level: &mut LevelState) {
    level.stop_movers(line.tag);
}

/// EV_DoPlat. `amount` is the RaiseAndChange height delta.
pub fn ev_do_platform(
    line: MapPtr<LineDef>,
    kind: PlatKind,
    amount: i32,
    level: &mut LevelState,
) -> bool {
    let mut ret = false;

    if matches!(kind, PlatKind::PerpetualRaise) {
        level.activate_movers_in_stasis(line.tag);
    }

    let mut spawned = Vec::new();

    for sector in level
        .level_data
        .sectors
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        if sector.specialdata.is_some() {
            continue;
        }
        ret = true;

        let mut sec = MapPtr::new(sector);
        let mut plat = base_mover(sec.clone(), MoverPlane::Floor, PLATSPEED);
        plat.tag = line.tag;
        plat.stoppable = true;
        plat.sounds = plat_sounds();
        // A lift that traps something on the way up drops back down.
        plat.crush_response = CrushResponse::Reverse;

        let (low, high) = plat_target(sec.clone(), kind, amount);
        plat.low = MoveTarget::Fixed(low);
        plat.high = high;

        match kind {
            PlatKind::RaiseToNearestAndChange | PlatKind::RaiseAndChange => {
                plat.speed = PLATSPEED / 2;
                plat.base_speed = plat.speed;
                plat.phase = MoverPhase::Up;
                plat.sounds.moving = Some(SfxName::Stnmov);
                if matches!(kind, PlatKind::RaiseToNearestAndChange) {
                    sec.special = SectorSpecial::default();
                }
                sec.floorpic = line.frontsector.floorpic;
                level
                    .level_data
                    .bsp_3d
                    .update_flat_texture(sec.num as usize, MovementType::Floor);
                start_sector_sound(&line, SfxName::Stnmov, &level.snd_command);
            }
            PlatKind::PerpetualRaise => {
                plat.wait = TICRATE * PLATWAIT;
                plat.at_top = ArrivalAction::WaitThenReverse;
                plat.at_bottom = ArrivalAction::WaitThenReverse;
                plat.phase = if (p_random() & 1) == 0 {
                    MoverPhase::Up
                } else {
                    MoverPhase::Down
                };
                start_sector_sound(&line, SfxName::Pstart, &level.snd_command);
            }
            PlatKind::DownWaitUpStay | PlatKind::BlazeDWUS => {
                plat.speed = if matches!(kind, PlatKind::BlazeDWUS) {
                    PLATSPEED * 8
                } else {
                    PLATSPEED * 4
                };
                plat.base_speed = plat.speed;
                plat.wait = TICRATE * PLATWAIT;
                plat.at_bottom = ArrivalAction::WaitThenReverse;
                plat.phase = MoverPhase::Down;
                start_sector_sound(&line, SfxName::Pstart, &level.snd_command);
            }
        }

        if let Some(ptr) = push_mover(plat, &mut level.thinkers) {
            spawned.push(ptr);
        }
    }

    for plat in spawned {
        level.add_active_mover(plat);
    }
    ret
}
