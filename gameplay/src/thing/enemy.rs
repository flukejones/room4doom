//! ENEMY THINKING
//! Enemies are always spawned with targetplayer = -1, threshold = 0.
//!
//! Most monsters are spawned unaware of all players, but some can be made aware
//! on spawn.
//!
//! Doom source name `p_enemy`

#[cfg(feature = "hprof")]
use coarse_prof::profile;
use log::trace;
use sound_common::SfxName;
use std::ptr;

use crate::bsp_trace::PortalZ;
use crate::doom_def::MISSILERANGE;
use crate::env::doors::{DoorKind, ev_do_door};
use crate::env::floor::{FloorKind, ev_do_floor};
use crate::info::{MOBJINFO, StateNum};
use crate::level::LevelState;
use crate::thing::{MapObjFlag, MapObject, MoveDir};
use crate::thinker::{Thinker, ThinkerData};
use crate::{MAXPLAYERS, MapObjKind, SectorExt, teleport_move};
use game_config::{GameMode, Skill};
use level::map_defs::{LineDef, SlopeType};
use level::{LineDefFlags, MapPtr, Sector};
use math::{
    ANG45, ANG90, ANG180, ANG270, Angle, Bam, FixedT, float_to_fixed, p_aprox_distance, p_random, r_point_to_angle
};

use super::movement::SubSectorMinMax;

/// OG Doom xspeed/yspeed tables for movedir (raw fixed-point 16.16).
/// Order: East, NorthEast, North, NorthWest, West, SouthWest, South, SouthEast.
const DIR_XSPEED_RAW: [i32; 8] = [65536, 47000, 0, -47000, -65536, -47000, 0, 47000];
const DIR_YSPEED_RAW: [i32; 8] = [0, 47000, 65536, 47000, 0, -47000, -65536, -47000];
/// Max angle change per tic for revenant tracer missile homing (OG p_enemy.c).
const TRACEANGLE: u32 = 0xC000000;

/// This was only ever called with the player as the target, so it never follows
/// the original comment stating that if a monster yells it alerts surrounding
/// monsters
pub(crate) fn noise_alert(target: &mut MapObject) {
    let vc = unsafe {
        (*target.level).valid_count += 1;
        (*target.level).valid_count
    };
    let sect = target.subsector.sector.clone();
    sound_flood(sect, vc, 0, target);
}

fn sound_flood(
    mut sector: MapPtr<Sector>,
    valid_count: usize,
    sound_blocks: i32,
    target: &mut MapObject,
) {
    if sector.validcount == valid_count && sector.soundtraversed <= sound_blocks + 1 {
        return; // already flooded
    }

    sector.validcount = valid_count;
    sector.soundtraversed = sound_blocks + 1;
    sector.set_sound_target_thinker(target.thinker);

    for line in sector.lines.iter() {
        if !line.flags.contains(LineDefFlags::TwoSided) {
            continue;
        }

        if PortalZ::new(line).range <= 0 {
            continue; // A door, and it's closed
        }

        let sector = if ptr::eq(line.front_sidedef.sector.as_ref(), sector.as_ref()) {
            unsafe { line.back_sidedef.as_ref().unwrap_unchecked().sector.clone() }
        } else {
            line.front_sidedef.sector.clone()
        };

        if line.flags.contains(LineDefFlags::BlockSound) {
            if sound_blocks == 0 {
                sound_flood(sector, valid_count, 1, target);
            }
        } else {
            sound_flood(sector, valid_count, sound_blocks, target);
        }
    }
}

/// A_FaceTarget
pub(crate) fn a_facetarget(actor: &mut MapObject) {
    actor.flags.remove(MapObjFlag::Ambush);

    let ax = actor.x;
    let ay = actor.y;
    let mut angle = actor.angle;
    if let Some(target) = actor.target_mut() {
        let dx = target.x - ax;
        let dy = target.y - ay;
        angle = Angle::from_bam(r_point_to_angle(dx, dy));
        if target.flags.contains(MapObjFlag::Shadow) {
            let fuzz = ((p_random() - p_random()) << 21) as u32;
            angle = angle + Angle::from_bam(fuzz);
        }
    }
    actor.angle = angle;
}

/// Actor has a melee attack,
/// so it tries to close as fast as possible
pub(crate) fn a_chase(actor: &mut MapObject) {
    #[cfg(feature = "hprof")]
    profile!("a_chase");
    if actor.reactiontime > 0 {
        actor.reactiontime -= 1;
    }

    // modify target threshold
    if actor.threshold > 0 {
        if let Some(target) = actor.target {
            let target = unsafe { (*target).mobj() };

            if target.health <= 0 {
                actor.threshold = 0;
            } else {
                actor.threshold -= 1;
            }
        } else {
            actor.threshold = 0;
        }
    }

    if actor.movedir < MoveDir::None {
        // OG: actor->angle &= (7<<29) — masks angle IN PLACE, snapping to nearest 45°
        let snapped = actor.angle.to_bam() & (7 << 29);
        actor.angle = Angle::from_bam(snapped);
        let target_bam = (actor.movedir as u32) << 29;
        let delta = snapped.wrapping_sub(target_bam) as i32;
        if delta > 0 {
            actor.angle = Angle::from_bam(snapped.wrapping_sub(ANG45));
        } else if delta < 0 {
            actor.angle = Angle::from_bam(snapped.wrapping_add(ANG45));
        }
    }

    // If already have a target check if proper, else look for it
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj() };
        // Inanimate object, try to find new target
        if !target.flags.contains(MapObjFlag::Shootable) {
            if actor.look_for_players(true) {
                return; // Found a new target
            }
            actor.set_state(actor.info.spawnstate);
            return;
        }
    } else {
        if actor.look_for_players(true) {
            return; // Found a new target
        }
        actor.set_state(actor.info.spawnstate);
        return;
    }

    if actor.flags.contains(MapObjFlag::Justattacked) {
        actor.flags.remove(MapObjFlag::Justattacked);
        // TODO: if (gameskill != sk_nightmare && !fastparm)
        let skill = unsafe { (*actor.level).options.skill };
        if skill != Skill::Nightmare {
            actor.new_chase_dir();
        }
        return;
    }

    // Melee attack?
    if actor.info.meleestate != StateNum::None && actor.check_melee_range() {
        if actor.info.attacksound != SfxName::None {
            actor.start_sound(actor.info.attacksound);
        }
        actor.set_state(actor.info.meleestate);
        return;
    }

    // Missile attack?
    // OG: if (gameskill < sk_nightmare && !fastparm && actor->movecount) goto
    // nomissile; r4d has no fastparm, so: skip if non-nightmare AND movecount >
    // 0
    if actor.info.missilestate != StateNum::None {
        let skill = unsafe { (*actor.level).options.skill };
        // OG: skip if (gameskill < nightmare && movecount != 0)
        if skill == Skill::Nightmare || actor.movecount == 0 {
            if actor.check_missile_range() {
                actor.set_state(actor.info.missilestate);
                actor.flags.insert(MapObjFlag::Justattacked);
                return;
            }
        }
    }

    // nomissile:
    // // possibly choose another target
    // if (netgame && !actor->threshold && !P_CheckSight(actor, actor->target))
    // {
    // if (P_LookForPlayers(actor, true))
    // return; // got a new target
    // }

    // // chase towards player
    actor.movecount -= 1;
    let moved = actor.movecount >= 0 && actor.do_move();
    if !moved {
        actor.new_chase_dir();
    }

    // make active sound
    if actor.info.activesound != SfxName::None && p_random() < 3 {
        actor.start_sound(actor.info.activesound);
    }
}

/// Stay in this state until a player is sighted.
pub(crate) fn a_look(actor: &mut MapObject) {
    #[cfg(feature = "hprof")]
    profile!("a_look");
    actor.threshold = 0;
    // TODO: any shot will wake up
    // if let Some(target) = actor.target {
    //     let target = &*target;
    //     if target.health <= 0 {
    //         actor.set_state(actor.info.spawnstate);
    //         return;
    //     }
    // }

    let ss = actor.subsector.clone();
    let mut goto_seeyou = false;
    if let Some(target) = ss.sector.sound_target() {
        if target.flags.contains(MapObjFlag::Shootable) {
            actor.target = actor.subsector.sector.sound_target_raw();

            if actor.flags.contains(MapObjFlag::Ambush) {
                if actor.check_sight_target(target) {
                    goto_seeyou = true;
                }
                // ambush + no sight: fall through to look_for_players
            } else {
                goto_seeyou = true;
            }
        }
    }
    if !goto_seeyou && !actor.look_for_players(false) {
        return;
    }

    if actor.info.seesound != SfxName::None {
        let sound = match actor.info.seesound {
            SfxName::Posit1 | SfxName::Posit2 | SfxName::Posit3 => {
                SfxName::from((SfxName::Posit1 as i32 + p_random() % 3) as u8)
            }
            SfxName::Bgsit1 | SfxName::Bgsit2 => {
                SfxName::from((SfxName::Bgsit1 as i32 + p_random() % 2) as u8)
            }
            _ => actor.info.seesound,
        };

        // if actor.kind == MapObjKind::MT_SPIDER || actor.kind == MapObjKind::MT_CYBORG
        // {     // TODO: FULL VOLUME!
        //     actor.start_sound(sound);
        // } else {
        //     actor.start_sound(sound);
        // }
        actor.start_sound(sound);
    }

    actor.set_state(actor.info.seestate);
}

pub(crate) fn a_fire(actor: &mut MapObject) {
    #[cfg(feature = "hprof")]
    profile!("a_fire");
    if let Some(dest) = actor.tracer {
        let dest = unsafe { (*dest).mobj() };
        if let Some(targ) = actor.target_mut() {
            // don't move it if the vile lost sight
            if !targ.check_sight_target(dest) {
                return;
            }

            unsafe { actor.unset_thing_position() };
            let dist = FixedT::from_f32(24.0);
            let bam = dest.angle.to_bam();
            actor.x = dest.x + dist.fixed_mul(FixedT::cos_bam(bam));
            actor.y = dest.y + dist.fixed_mul(FixedT::sin_bam(bam));
            actor.z = dest.z;
            unsafe { actor.set_thing_position() };
        }
    }
}

pub(crate) fn a_scream(actor: &mut MapObject) {
    let sound = match actor.info.deathsound {
        SfxName::None => return,
        SfxName::Podth1 | SfxName::Podth2 | SfxName::Podth3 => {
            SfxName::from(SfxName::Podth1 as u8 + (p_random() % 3) as u8)
        }
        SfxName::Bgdth1 | SfxName::Bgdth2 => {
            SfxName::from(SfxName::Bgdth1 as u8 + (p_random() % 2) as u8)
        }
        _ => SfxName::from(actor.info.deathsound as u8),
    };

    // Check for bosses.
    if matches!(actor.kind, MapObjKind::MT_SPIDER | MapObjKind::MT_CYBORG) {
        // full volume
        // TODO: start_sound("a_scream", None, sound);
    } else {
        actor.start_sound(sound);
    }
}

pub(crate) fn a_fall(actor: &mut MapObject) {
    // actor is on ground, it can be walked over
    actor.flags.remove(MapObjFlag::Solid);
    // So change this if corpse objects are meant to be obstacles.
}

pub(crate) fn a_explode(actor: &mut MapObject) {
    actor.radius_attack(128);
}

pub(crate) fn a_xscream(actor: &mut MapObject) {
    actor.start_sound(SfxName::Slop);
}

pub(crate) fn a_keendie(actor: &mut MapObject) {
    a_fall(actor);

    // TODO: ev_do_door takes &mut LevelState<f32>, narrow transmute until
    // LevelState is fully generic
    let level: &mut LevelState =
        unsafe { &mut *(actor.level as *mut LevelState as *mut LevelState) };
    // Check keens are all dead
    let mut dead = true;
    level.thinkers.run_fn_on_things(|thinker| {
        if let &ThinkerData::MapObject(ref mobj) = thinker.data() {
            if !ptr::eq(
                mobj as *const _ as *const (),
                actor as *const _ as *const (),
            ) && mobj.kind == actor.kind
                && mobj.health > 0
            {
                dead = false;
            }
        }
        true
    });
    if !dead {
        return;
    };

    let sidedef = actor.subsector.sector.lines[0].front_sidedef.clone();
    let sector = actor.subsector.sector.clone();

    let mut junk = LineDef {
        num: 0,
        v1: unsafe { MapPtr::new_null() },
        v2: unsafe { MapPtr::new_null() },
        delta: Default::default(),
        delta_fp: [0; 2],
        flags: LineDefFlags::empty(),
        special: 0,
        tag: 666,
        bbox: Default::default(),
        bbox_int: [0; 4],
        slopetype: SlopeType::Horizontal,
        front_sidedef: sidedef,
        back_sidedef: None,
        frontsector: sector,
        backsector: None,
        valid_count: 0,
        sides: [0, 0],
        default_special: 0,
        default_tag: 0,
    };
    ev_do_door(MapPtr::new(&mut junk), DoorKind::BlazeOpen, level);
}

pub(crate) fn a_hoof(actor: &mut MapObject) {
    actor.start_sound(SfxName::Hoof);
    a_chase(actor);
}

pub(crate) fn a_metal(actor: &mut MapObject) {
    actor.start_sound(SfxName::Metal);
    a_chase(actor);
}

pub(crate) fn a_babymetal(actor: &mut MapObject) {
    actor.start_sound(SfxName::Bspwlk);
    a_chase(actor);
}

pub(crate) fn a_brainawake(actor: &mut MapObject) {
    let mut targets = Vec::new();
    actor.level().thinkers.find_thinker_mut(|t| {
        if t.is_mobj() {
            // todo: store pointers....
            // since these are level objects they should
            // never have their memory location move
            if t.mobj().kind == MapObjKind::MT_BOSSTARGET {
                // eeeesssh...
                // TODO: fix this
                targets.push(t as *const Thinker as *mut Thinker)
            }
        }
        false
    });
    actor.boss_targets = targets;
}

pub(crate) fn a_braindie(actor: &mut MapObject) {
    actor.level_mut().do_exit_level();
}

pub(crate) fn a_brainspit(actor: &mut MapObject) {
    let skill = unsafe { (*actor.level).options.skill };
    if skill == Skill::Baby {
        return;
    }
    // spooge a cube at the thing
    let target_thinker = unsafe { &mut (*actor.boss_targets[actor.boss_target_on]) };
    actor.boss_target_on = (actor.boss_target_on + 1) % actor.boss_targets.len();

    let target_y = target_thinker.mobj_mut().y;
    let level = unsafe { &mut *actor.level };
    let m = MapObject::spawn_missile(
        actor,
        target_thinker.mobj_mut(),
        MapObjKind::MT_SPAWNSHOT,
        level,
    );
    m.target = Some(target_thinker);
    let dy = target_y - actor.y;
    m.reactiontime = (dy / m.momy).to_i32() / m.state.tics;

    actor.start_sound(SfxName::Bospit);
}

pub(crate) fn a_brainpain(actor: &mut MapObject) {
    actor.start_sound(SfxName::Bospn);
}

pub(crate) fn a_brainscream(actor: &mut MapObject) {
    let actor_x_raw = actor.x.to_fixed_raw();
    let actor_y_raw = actor.y.to_fixed_raw();
    let mut x_raw = actor_x_raw - 196 * 0x10000_i32;
    while x_raw < actor_x_raw + 320 * 0x10000_i32 {
        let y_raw = actor_y_raw - 320 * 0x10000_i32;
        let z_raw = (128 + p_random() * 2) * 0x10000_i32;
        let level = unsafe { &mut *actor.level };
        let th = MapObject::spawn_map_object(
            FixedT::from_fixed(x_raw),
            FixedT::from_fixed(y_raw),
            FixedT::from_fixed(z_raw),
            MapObjKind::MT_ROCKET,
            level,
        );
        unsafe {
            let th = &mut (*th);
            th.momz = FixedT::from_fixed(p_random() * 512);
            th.set_state(StateNum::BRAINEXPLODE1);
            th.tics -= p_random() & 7;
            if th.tics < 1 {
                th.tics = 1;
            }
        }
        x_raw += 8 * 0x10000_i32;
    }
    actor.start_sound(SfxName::Bosdth);
}

pub(crate) fn a_brainexplode(actor: &mut MapObject) {
    let x_raw = actor.x.to_fixed_raw() + (p_random() - p_random()) * 2048;

    let z_raw = (128 + p_random() * 2) * 0x10000_i32;
    let level = unsafe { &mut *actor.level };
    let th = MapObject::spawn_map_object(
        FixedT::from_fixed(x_raw),
        actor.y,
        FixedT::from_fixed(z_raw),
        MapObjKind::MT_ROCKET,
        level,
    );
    unsafe {
        let th = &mut (*th);
        th.momz = FixedT::from_fixed(p_random() * 512);
        th.set_state(StateNum::BRAINEXPLODE1);
        th.tics -= p_random() & 7;
        if th.tics < 1 {
            th.tics = 1;
        }
    }
}

pub(crate) fn a_spawnfly(actor: &mut MapObject) {
    actor.reactiontime -= 1;
    if actor.reactiontime > 0 {
        return;
    }

    let level = unsafe { &mut *actor.level };
    if let Some(target) = actor.target() {
        let target_x = target.x;
        let target_y = target.y;
        let fog = unsafe {
            &mut *MapObject::spawn_map_object(
                target_x,
                target_y,
                target.z,
                MapObjKind::MT_SPAWNFIRE,
                level,
            )
        };
        fog.start_sound(SfxName::Telept);

        let r = p_random();
        let t = if r < 50 {
            MapObjKind::MT_TROOP
        } else if r < 90 {
            MapObjKind::MT_SERGEANT
        } else if r < 120 {
            MapObjKind::MT_SHADOWS
        } else if r < 130 {
            MapObjKind::MT_PAIN
        } else if r < 160 {
            MapObjKind::MT_HEAD
        } else if r < 162 {
            MapObjKind::MT_VILE
        } else if r < 172 {
            MapObjKind::MT_UNDEAD
        } else if r < 192 {
            MapObjKind::MT_BABY
        } else if r < 222 {
            MapObjKind::MT_FATSO
        } else if r < 246 {
            MapObjKind::MT_KNIGHT
        } else {
            MapObjKind::MT_BRUISER
        };

        let new_critter =
            unsafe { &mut *MapObject::spawn_map_object(target_x, target_y, target.z, t, level) };
        if new_critter.look_for_players(true) {
            new_critter.set_state(new_critter.info.seestate);
        }
        teleport_move(target_x, target_y, new_critter, level);
        actor.remove();
    }
}

pub(crate) fn a_spawnsound(actor: &mut MapObject) {
    actor.start_sound(SfxName::Boscub);
    a_spawnfly(actor);
}

pub(crate) fn a_vilestart(actor: &mut MapObject) {
    actor.start_sound(SfxName::Vilatk);
}

fn vile_raise_check(actor: &mut MapObject, obj: &mut MapObject) -> bool {
    if !obj.flags.contains(MapObjFlag::Corpse) {
        return true; // not a monster
    }

    if obj.tics != -1 {
        return true; // not lying still yet
    }

    if obj.info.raisestate == StateNum::None {
        return true; // monster doesn't have a raise state
    }

    let max_dist = obj.radius + actor.radius;
    // OG Doom: viletryx = vile->x + vile->info->speed * xspeed[movedir]
    let dir = actor.movedir as usize;
    let tryx = actor.x + FixedT::from_fixed(actor.info.speed * DIR_XSPEED_RAW[dir]);
    let tryy = actor.y + FixedT::from_fixed(actor.info.speed * DIR_YSPEED_RAW[dir]);
    // OG Doom uses per-axis abs check, not Euclidean distance
    if (obj.x - tryx).doom_abs() > max_dist || (obj.y - tryy).doom_abs() > max_dist {
        return true;
    }

    obj.momx = FixedT::ZERO;
    obj.momy = FixedT::ZERO;
    let old_height = obj.height;
    obj.height = FixedT::from_f32(obj.info.height);
    let mut ctrl = SubSectorMinMax::default();
    let check = obj.p_check_position(obj.x, obj.y, &mut ctrl);
    obj.height = old_height;
    if !check {
        return true;
    }

    false
}

pub(crate) fn a_vilechase(actor: &mut MapObject) {
    if actor.movedir != MoveDir::None {
        // look for corpses
        let mut ss = actor.subsector.clone();
        let res = ss.sector.run_mut_func_on_thinglist(|obj| {
            // Check corpses are within radius
            if !vile_raise_check(actor, obj) {
                // found one so raise it
                let last_target = actor.target.take();
                actor.target = Some(obj.thinker);
                a_facetarget(actor);
                actor.target = last_target;

                actor.set_state(StateNum::VILE_HEAL1);
                actor.start_sound(SfxName::Slop);
                // info = corpsehit->info;

                obj.set_state(obj.info.raisestate);
                obj.height = obj.height * 2;
                obj.flags = obj.info.flags;
                obj.health = obj.info.spawnhealth;
                obj.target = None;
                return false;
            }
            true
        });
        if !res {
            // found a corpse so return
            trace!("Archvile found a corpse to raise");
            return;
        }
    }

    a_chase(actor);
}

pub(crate) fn a_viletarget(actor: &mut MapObject) {
    if let Some(targ) = actor.target {
        let targ = unsafe { (*targ).mobj_mut() };
        a_facetarget(actor);

        let level = unsafe { &mut *actor.level };
        let fog = MapObject::spawn_map_object(targ.x, targ.y, targ.z, MapObjKind::MT_FIRE, level);
        let fog = unsafe { &mut *fog };
        actor.tracer = Some(fog.thinker); // actor/vile owns the fire
        fog.target = Some(actor.thinker); // fire target is vile so the fire can check its owner
        fog.tracer = actor.target;
        a_fire(fog);
    }
}

pub(crate) fn a_vileattack(actor: &mut MapObject) {
    if let Some(targ) = actor.target {
        let targ = unsafe { (*targ).mobj_mut() };
        a_facetarget(actor);

        if !actor.check_sight_target(targ) {
            return;
        }

        actor.start_sound(SfxName::Barexp);
        targ.p_take_damage(Some((actor.x, actor.y, actor.z)), Some(actor), 20);
        targ.momz = FixedT::from_fixed(1000 * 0x10000_i32 / targ.info.mass);

        let bam = actor.angle.to_bam();
        if let Some(fire) = actor.tracer {
            let fire = unsafe { (*fire).mobj_mut() };
            let dist24 = FixedT::from_f32(24.0);
            fire.x = targ.x - dist24.fixed_mul(FixedT::cos_bam(bam));
            fire.y = targ.y - dist24.fixed_mul(FixedT::sin_bam(bam));
            fire.radius_attack(70);
        }
    }
}

pub(crate) fn a_posattack(actor: &mut MapObject) {
    if actor.target.is_none() {
        return;
    }

    a_facetarget(actor);
    let distance: FixedT = MISSILERANGE.into();
    let mut bsp_trace = actor.get_shoot_bsp_trace(distance);
    let slope = actor.aim_line_attack(distance, &mut bsp_trace);

    actor.start_sound(SfxName::Pistol);

    // OG: angle += (P_Random()-P_Random())<<20
    let spread = ((p_random() - p_random()) << 20) as u32;
    let angle = Angle::from_bam(actor.angle.to_bam().wrapping_add(spread));
    let damage = ((p_random() % 5) + 1) * 3;
    actor.line_attack(damage, distance, angle, slope, &mut bsp_trace);
}

pub(crate) fn a_sposattack(actor: &mut MapObject) {
    if actor.target.is_none() {
        return;
    }

    a_facetarget(actor);
    let distance: FixedT = MISSILERANGE.into();
    let mut bsp_trace = actor.get_shoot_bsp_trace(distance);
    let slope = actor.aim_line_attack(distance, &mut bsp_trace);

    actor.start_sound(SfxName::Shotgn);

    for _ in 0..3 {
        // OG: angle = bangle + ((P_Random()-P_Random())<<20)
        let spread = ((p_random() - p_random()) << 20) as u32;
        let angle = Angle::from_bam(actor.angle.to_bam().wrapping_add(spread));
        let damage = ((p_random() % 5) + 1) * 3;
        actor.line_attack(damage, distance, angle, slope.clone(), &mut bsp_trace);
    }
}

pub(crate) fn a_cposattack(actor: &mut MapObject) {
    if actor.target.is_none() {
        return;
    }

    a_facetarget(actor);
    let distance: FixedT = MISSILERANGE.into();
    let mut bsp_trace = actor.get_shoot_bsp_trace(distance);
    let slope = actor.aim_line_attack(distance, &mut bsp_trace);

    actor.start_sound(SfxName::Shotgn);

    // OG: angle = bangle + ((P_Random()-P_Random())<<20)
    let spread = ((p_random() - p_random()) << 20) as u32;
    let angle = Angle::from_bam(actor.angle.to_bam().wrapping_add(spread));
    let damage = ((p_random() % 5) + 1) * 3;
    actor.line_attack(damage, distance, angle, slope, &mut bsp_trace);
}

pub(crate) fn a_bspiattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj_mut() };
        a_facetarget(actor);

        let level = unsafe { &mut *actor.level };
        MapObject::spawn_missile(actor, target, MapObjKind::MT_ARACHPLAZ, level);
    }
}

pub(crate) fn a_skullattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj() };

        a_facetarget(actor);
        actor.flags.insert(MapObjFlag::Skullfly);
        actor.start_sound(actor.info.attacksound);

        let dx = target.x - actor.x;
        let dy = target.y - actor.y;
        actor.angle = Angle::from_bam(r_point_to_angle(dx, dy));
        let bam = actor.angle.to_bam();
        let speed = FixedT::from_fixed(actor.info.speed);
        actor.momx = speed.fixed_mul(FixedT::cos_bam(bam));
        actor.momy = speed.fixed_mul(FixedT::sin_bam(bam));

        // OG Doom: dist = P_AproxDistance(...) / SKULLSPEED
        let dx = target.x - actor.x;
        let dy = target.y - actor.y;
        let speed = FixedT::from_fixed(actor.info.speed);
        let adist = p_aprox_distance(dx, dy);
        let mut dist = adist / speed;
        if dist < FixedT::ONE {
            dist = FixedT::ONE;
        }
        let half_height = target.height.shr(1);
        actor.momz = (target.z + half_height - actor.z) / dist;
    }
}

pub(crate) fn a_headattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj_mut() };
        a_facetarget(actor);

        if actor.check_melee_range() {
            actor.start_sound(SfxName::Claw);
            let damage = ((p_random() % 8) + 1) * 10;
            target.p_take_damage(Some((actor.x, actor.y, actor.z)), Some(actor), damage);
            return;
        }

        let level = unsafe { &mut *actor.level };
        MapObject::spawn_missile(actor, target, MapObjKind::MT_BRUISERSHOT, level);
    }
}

pub(crate) fn a_sargattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj_mut() };

        a_facetarget(actor);
        if actor.check_melee_range() {
            let damage = ((p_random() % 10) + 1) * 4;
            target.p_take_damage(Some((actor.x, actor.y, actor.z)), Some(actor), damage);
        }
    }
}

pub(crate) fn a_bruisattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj_mut() };
        a_facetarget(actor);

        if actor.check_melee_range() {
            let damage = ((p_random() % 6) + 1) * 10;
            target.p_take_damage(Some((actor.x, actor.y, actor.z)), Some(actor), damage);
            return;
        }

        let level = unsafe { &mut *actor.level };
        MapObject::spawn_missile(actor, target, MapObjKind::MT_HEADSHOT, level);
    }
}

pub(crate) fn a_cposrefire(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        a_facetarget(actor);
        if p_random() < 40 {
            return;
        }

        let target = unsafe { (*target).mobj_mut() };
        if target.health <= 0 || !actor.check_sight_target(target) {
            actor.set_state(actor.info.seestate);
        }
    }
}

pub(crate) fn a_cyberattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        a_facetarget(actor);
        let target = unsafe { (*target).mobj_mut() };

        let level = unsafe { &mut *actor.level };
        MapObject::spawn_missile(actor, target, MapObjKind::MT_ROCKET, level);
    }
}

pub(crate) fn a_troopattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        a_facetarget(actor);

        let target = unsafe { (*target).mobj_mut() };

        if actor.check_melee_range() {
            actor.start_sound(SfxName::Claw);
            let damage = ((p_random() % 8) + 1) * 3;
            target.p_take_damage(Some((actor.x, actor.y, actor.z)), Some(actor), damage);
            return;
        }

        let level = unsafe { &mut *actor.level };
        MapObject::spawn_missile(actor, target, MapObjKind::MT_TROOPSHOT, level);
    }
}

pub(crate) fn a_pain(actor: &mut MapObject) {
    if actor.info.painsound != SfxName::None {
        actor.start_sound(actor.info.painsound);
    }
}

fn a_painshootskull(actor: &mut MapObject, angle: Angle<Bam>) {
    a_facetarget(actor);
    // TODO: limit amount of skulls

    let bam = angle.to_bam();
    // OG: prestep = 4*FRACUNIT + 3*(actor->info->radius +
    // mobjinfo[MT_SKULL].radius)/2
    let skull_radius_raw = float_to_fixed(MOBJINFO[MapObjKind::MT_SKULL as usize].radius);
    let prestep_raw = 4 * 0x10000_i32 + 3 * (actor.radius.to_fixed_raw() + skull_radius_raw) / 2;
    let prestep = FixedT::from_fixed(prestep_raw);
    let spawn_x = actor.x + prestep.fixed_mul(FixedT::cos_bam(bam));
    let spawn_y = actor.y + prestep.fixed_mul(FixedT::sin_bam(bam));
    let spawn_z = actor.z + 8;

    let level = unsafe { &mut *actor.level };
    unsafe {
        let skull = &mut (*MapObject::spawn_map_object(
            spawn_x,
            spawn_y,
            spawn_z,
            MapObjKind::MT_SKULL,
            level,
        ));
        let mut ctrl = SubSectorMinMax::default();
        if !skull.p_try_move(skull.x, skull.y, &mut ctrl) {
            // OG: P_DamageMobj(newmobj, actor, actor, 10000)
            skull.p_take_damage(Some((actor.x, actor.y, actor.z)), Some(actor), 10000);
            return;
        }
        skull.target = actor.target;
        a_skullattack(skull);
    }
}

pub(crate) fn a_painattack(actor: &mut MapObject) {
    a_facetarget(actor);
    a_painshootskull(actor, actor.angle);
}

pub(crate) fn a_paindie(actor: &mut MapObject) {
    a_fall(actor);
    let bam = actor.angle.to_bam();
    a_painshootskull(actor, Angle::from_bam(bam.wrapping_add(ANG90)));
    a_painshootskull(actor, Angle::from_bam(bam.wrapping_add(ANG180)));
    a_painshootskull(actor, Angle::from_bam(bam.wrapping_add(ANG270)));
}

/// OG: `FATSPREAD = ANG90/8 = 0x08000000`
const FAT_SPREAD_BAM: u32 = ANG90 / 8;

pub(crate) fn a_fatattack1(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let level = unsafe { &mut *actor.level };
        let target = unsafe { (*target).mobj_mut() };

        a_facetarget(actor);
        actor.angle = Angle::from_bam(actor.angle.to_bam().wrapping_add(FAT_SPREAD_BAM));
        // 1 away
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_FATSHOT, level);
        let bam = missile.angle.to_bam();
        let speed = FixedT::from_fixed(missile.info.speed);
        missile.momx = speed.fixed_mul(FixedT::cos_bam(bam));
        missile.momy = speed.fixed_mul(FixedT::sin_bam(bam));

        // 2 away
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_FATSHOT, level);
        actor.angle = Angle::from_bam(actor.angle.to_bam().wrapping_add(FAT_SPREAD_BAM));
        let bam = missile.angle.to_bam();
        let speed = FixedT::from_fixed(missile.info.speed);
        missile.momx = speed.fixed_mul(FixedT::cos_bam(bam));
        missile.momy = speed.fixed_mul(FixedT::sin_bam(bam));
    }
}
pub(crate) fn a_fatattack2(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let level = unsafe { &mut *actor.level };
        let target = unsafe { (*target).mobj_mut() };

        a_facetarget(actor);
        actor.angle = Angle::from_bam(actor.angle.to_bam().wrapping_sub(FAT_SPREAD_BAM));
        // 1 away
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_FATSHOT, level);
        let bam = missile.angle.to_bam();
        let speed = FixedT::from_fixed(missile.info.speed);
        missile.momx = speed.fixed_mul(FixedT::cos_bam(bam));
        missile.momy = speed.fixed_mul(FixedT::sin_bam(bam));

        // 2 away
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_FATSHOT, level);
        actor.angle = Angle::from_bam(actor.angle.to_bam().wrapping_sub(2 * FAT_SPREAD_BAM));
        let bam = missile.angle.to_bam();
        let speed = FixedT::from_fixed(missile.info.speed);
        missile.momx = speed.fixed_mul(FixedT::cos_bam(bam));
        missile.momy = speed.fixed_mul(FixedT::sin_bam(bam));
    }
}
pub(crate) fn a_fatattack3(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let level = unsafe { &mut *actor.level };
        let target = unsafe { (*target).mobj_mut() };

        a_facetarget(actor);
        actor.angle = Angle::from_bam(actor.angle.to_bam().wrapping_sub(FAT_SPREAD_BAM / 2));
        // 1 away
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_FATSHOT, level);
        let bam = missile.angle.to_bam();
        let speed = FixedT::from_fixed(missile.info.speed);
        missile.momx = speed.fixed_mul(FixedT::cos_bam(bam));
        missile.momy = speed.fixed_mul(FixedT::sin_bam(bam));

        // 2 away
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_FATSHOT, level);
        actor.angle = Angle::from_bam(actor.angle.to_bam().wrapping_add(FAT_SPREAD_BAM / 2));
        let bam = missile.angle.to_bam();
        let speed = FixedT::from_fixed(missile.info.speed);
        missile.momx = speed.fixed_mul(FixedT::cos_bam(bam));
        missile.momy = speed.fixed_mul(FixedT::sin_bam(bam));
    }
}

pub(crate) fn a_fatraise(actor: &mut MapObject) {
    a_facetarget(actor);
    actor.start_sound(SfxName::Manatk);
}

pub(crate) fn a_spidrefire(actor: &mut MapObject) {
    a_facetarget(actor);
    if p_random() < 10 {
        return;
    }
    let should_idle = actor.target.map_or(true, |t| {
        let t = unsafe { (*t).mobj_mut() };
        t.health <= 0 || !actor.check_sight_target(t)
    });
    if should_idle {
        actor.set_state(actor.info.seestate);
    }
}

pub(crate) fn a_bossdeath(actor: &mut MapObject) {
    let level: &mut LevelState =
        unsafe { &mut *(actor.level as *mut LevelState as *mut LevelState) };
    let map = level.options.map;
    let episode = level.options.episode;
    let mode = level.game_mode;
    let mt = actor.kind;

    // UMAPINFO boss actions override defaults
    if let Some(actions) = level.boss_actions.clone() {
        match actions {
            wad::umapinfo::BossActions::Clear => return,
            wad::umapinfo::BossActions::Actions(list) => {
                let matched: Vec<_> = list
                    .iter()
                    .filter(|a| MapObjKind::from_zdoom_name(&a.thing_type) == Some(mt))
                    .collect();
                if matched.is_empty() {
                    return;
                }
                if !all_bosses_dead(actor, level) {
                    return;
                }
                for action in &matched {
                    trigger_boss_line_special(action.line_special, action.tag as i16, actor, level);
                }
                return;
            }
        }
    }

    // Default boss death logic
    if mode == GameMode::Commercial {
        if map != 7 {
            return;
        }
        if mt != MapObjKind::MT_FATSO && mt != MapObjKind::MT_BABY {
            return;
        }
    } else {
        match episode {
            1 => {
                if map != 8 && mt != MapObjKind::MT_BRUISER {
                    return;
                }
            }
            2 => {
                if map != 8 && mt != MapObjKind::MT_CYBORG {
                    return;
                }
            }
            3 => {
                if map != 8 && mt != MapObjKind::MT_SPIDER {
                    return;
                }
            }
            4 => {
                if map != 6 && mt != MapObjKind::MT_CYBORG {
                    return;
                }
                if map != 8 && mt != MapObjKind::MT_SPIDER {
                    return;
                }
            }
            _ => {
                if map != 8 {
                    return;
                }
            }
        }
        for (i, p) in level.players().iter().enumerate() {
            if p.status.health > 0 {
                break;
            }
            if i == MAXPLAYERS - 1 {
                return;
            }
        }
    }

    if !all_bosses_dead(actor, level) {
        return;
    }

    let mut junk = make_boss_junk_linedef(actor, 666);

    if mode == GameMode::Commercial && map == 7 {
        if mt == MapObjKind::MT_FATSO {
            junk.tag = 666;
            ev_do_floor(MapPtr::new(&mut junk), FloorKind::LowerFloorToLowest, level);
            return;
        }
        if mt == MapObjKind::MT_BABY {
            junk.tag = 667;
            ev_do_floor(MapPtr::new(&mut junk), FloorKind::RaiseToTexture, level);
            return;
        }
    } else if episode == 1 {
        ev_do_floor(MapPtr::new(&mut junk), FloorKind::LowerFloorToLowest, level);
        return;
    } else if episode == 4 {
        if map == 6 {
            ev_do_door(MapPtr::new(&mut junk), DoorKind::BlazeOpen, level);
            return;
        } else if map == 8 {
            ev_do_floor(MapPtr::new(&mut junk), FloorKind::LowerFloorToLowest, level);
            return;
        }
    }

    level.do_completed();
}

fn all_bosses_dead(actor: &MapObject, level: &mut LevelState) -> bool {
    let mut dead = true;
    level.thinkers.run_fn_on_things(|thinker| {
        if let &ThinkerData::MapObject(ref mobj) = thinker.data() {
            if !ptr::eq(
                mobj as *const _ as *const (),
                actor as *const _ as *const (),
            ) && mobj.kind == actor.kind
                && mobj.health > 0
            {
                dead = false;
            }
        }
        true
    });
    dead
}

fn make_boss_junk_linedef(actor: &MapObject, tag: i16) -> LineDef {
    let sidedef = actor.subsector.sector.lines[0].front_sidedef.clone();
    let sector = actor.subsector.sector.clone();
    LineDef {
        num: 0,
        v1: unsafe { MapPtr::new_null() },
        v2: unsafe { MapPtr::new_null() },
        delta: Default::default(),
        delta_fp: [0; 2],
        flags: LineDefFlags::empty(),
        special: 0,
        tag,
        bbox: Default::default(),
        bbox_int: [0; 4],
        slopetype: SlopeType::Horizontal,
        front_sidedef: sidedef,
        back_sidedef: None,
        frontsector: sector,
        backsector: None,
        valid_count: 0,
        sides: [0, 0],
        default_special: 0,
        default_tag: 0,
    }
}

fn trigger_boss_line_special(special: i32, tag: i16, actor: &MapObject, level: &mut LevelState) {
    let mut junk = make_boss_junk_linedef(actor, tag);
    match special {
        // Tag 0 with certain specials = level exit
        11 if tag == 0 => level.do_completed(),
        51 if tag == 0 => level.do_secret_exit_level(),
        // Floor specials
        23 => {
            ev_do_floor(MapPtr::new(&mut junk), FloorKind::LowerFloorToLowest, level);
        }
        30 => {
            ev_do_floor(MapPtr::new(&mut junk), FloorKind::RaiseToTexture, level);
        }
        // Door specials
        29 => {
            ev_do_door(MapPtr::new(&mut junk), DoorKind::Normal, level);
        }
        105 => {
            ev_do_door(MapPtr::new(&mut junk), DoorKind::BlazeRaise, level);
        }
        108 => {
            ev_do_door(MapPtr::new(&mut junk), DoorKind::BlazeOpen, level);
        }
        _ => {
            log::warn!("UMAPINFO bossaction: unsupported line special {}", special);
        }
    }
}

pub(crate) fn a_skelwhoosh(actor: &mut MapObject) {
    if actor.target.is_some() {
        a_facetarget(actor);
        actor.start_sound(SfxName::Skeswg);
    }
}

pub(crate) fn a_skelfist(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        a_facetarget(actor);
        let target = unsafe { (*target).mobj_mut() };

        if actor.check_melee_range() {
            actor.start_sound(SfxName::Skepch);
            let damage = ((p_random() % 10) + 1) * 6;
            target.p_take_damage(Some((actor.x, actor.y, actor.z)), Some(actor), damage);
        }
    }
}

pub(crate) fn a_skelmissile(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj_mut() };
        a_facetarget(actor);

        let level = unsafe { &mut *actor.level };
        actor.z += 16;
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_TRACER, level);
        actor.z -= 16;

        missile.x += missile.momx;
        missile.y += missile.momy;
        missile.tracer = actor.target;
    }
}

/// Skelly missile that tracks the player/target
pub(crate) fn a_tracer(actor: &mut MapObject) {
    let level = unsafe { &mut *actor.level };
    if level.level_time & 3 != 0 {
        return;
    }
    // spawn a puff of smoke behind the rocket
    MapObject::spawn_puff(actor.x, actor.y, actor.z, FixedT::ZERO, level);
    let thing = MapObject::spawn_map_object(actor.x, actor.y, actor.z, MapObjKind::MT_SMOKE, level);
    let smoke = unsafe { &mut *thing };
    smoke.momz = FixedT::ONE;
    smoke.tics -= p_random() & 3;
    if smoke.tics < 1 {
        smoke.tics = 1;
    }

    if let Some(dest) = actor.tracer {
        let dest = unsafe { &mut *dest };
        if dest.mobj().health <= 0 {
            return;
        }

        // let delta = actor.angle.unit().angle_between(dest.mobj().angle.unit());
        // TODO: the slight adjustment if angle is greater than a limit

        let dx = dest.mobj().x - actor.x;
        let dy = dest.mobj().y - actor.y;
        let exact = r_point_to_angle(dx, dy);
        let angle_bam = actor.angle.to_bam();
        if exact != angle_bam {
            if exact.wrapping_sub(angle_bam) > 0x80000000 {
                actor.angle = Angle::from_bam(angle_bam.wrapping_sub(TRACEANGLE));
                if exact.wrapping_sub(actor.angle.to_bam()) < 0x80000000 {
                    actor.angle = Angle::from_bam(exact);
                }
            } else {
                actor.angle = Angle::from_bam(angle_bam.wrapping_add(TRACEANGLE));
                if exact.wrapping_sub(actor.angle.to_bam()) > 0x80000000 {
                    actor.angle = Angle::from_bam(exact);
                }
            }
        }
        let speed = FixedT::from_fixed(actor.info.speed);
        let clamped = actor.angle.to_bam();
        actor.momx = speed.fixed_mul(FixedT::cos_bam(clamped));
        actor.momy = speed.fixed_mul(FixedT::sin_bam(clamped));

        let dx = dest.mobj().x - actor.x;
        let dy = dest.mobj().y - actor.y;
        let adist = p_aprox_distance(dx, dy);
        let mut dist = adist / speed;
        if dist < FixedT::ONE {
            dist = FixedT::ONE;
        }
        let slope = (dest.mobj().z + 40 - actor.z) / dist;
        let eighth = FixedT::from_fixed(0x10000_i32 / 8);
        if slope < actor.momz {
            actor.momz -= eighth;
        } else {
            actor.momz += eighth;
        }
    }
}

pub(crate) fn a_startfire(actor: &mut MapObject) {
    actor.start_sound(SfxName::Flamst);
    a_fire(actor);
}

pub(crate) fn a_firecrackle(actor: &mut MapObject) {
    actor.start_sound(SfxName::Flame);
    a_fire(actor);
}

pub(crate) fn a_playerscream(actor: &mut MapObject) {
    let mut sound = SfxName::Pldeth;

    if actor.level().game_mode == GameMode::Commercial && actor.health < -50 {
        // IF THE PLAYER DIES LESS THAN -50% WITHOUT GIBBING
        sound = SfxName::Pdiehi;
    }

    actor.start_sound(sound);
}
