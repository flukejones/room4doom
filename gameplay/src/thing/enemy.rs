//! ENEMY THINKING
//! Enemies are always spawned with targetplayer = -1, threshold = 0.
//!
//! Most monsters are spawned unaware of all players, but some can be made aware
//! on spawn.
//!
//! Doom source name `p_enemy`

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use std::ptr;

use log::trace;
use sound_traits::SfxName;

use crate::doom_def::{MISSILERANGE, SKULLSPEED};
use crate::env::doors::{DoorKind, ev_do_door};
use crate::env::floor::{FloorKind, ev_do_floor};
use crate::info::{MOBJINFO, StateNum};
use crate::level::map_defs::{LineDef, SlopeType};
use crate::thing::{MapObjFlag, MapObject, MoveDir};
use crate::thinker::{Thinker, ThinkerData};
use crate::utilities::PortalZ;
use crate::{
    Angle, GameMode, LineDefFlags, MAXPLAYERS, MapObjKind, MapPtr, Sector, Skill, teleport_move
};
use math::{p_random, point_to_angle_2};

use super::movement::SubSectorMinMax;

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
    if sector.validcount == valid_count && sector.soundtraversed <= 2 {
        return; // Done with this sector, it's flooded
    }

    sector.validcount = valid_count;
    sector.soundtraversed = sound_blocks + 1;
    sector.set_sound_target(target.thinker);

    for line in sector.lines.iter() {
        if line.flags & LineDefFlags::TwoSided as u32 == 0 {
            continue;
        }

        if PortalZ::new(line).range <= 0.0 {
            continue; // A door, and it's closed
        }

        let sector = if ptr::eq(line.front_sidedef.sector.as_ref(), sector.as_ref()) {
            unsafe { line.back_sidedef.as_ref().unwrap_unchecked().sector.clone() }
        } else {
            line.front_sidedef.sector.clone()
        };

        if line.flags & LineDefFlags::BlockSound as u32 != 0 {
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
    actor.flags &= !(MapObjFlag::Ambush as u32);

    let xy = actor.xy;
    let mut angle = actor.angle;
    if let Some(target) = actor.target_mut() {
        angle = point_to_angle_2(target.xy, xy);
        if target.flags & MapObjFlag::Shadow as u32 == MapObjFlag::Shadow as u32 {
            actor.angle += (((p_random() - p_random()) >> 4) as f32).to_radians();
        }
    }
    actor.angle = angle;
}

/// Actor has a melee attack,
/// so it tries to close as fast as possible
pub(crate) fn a_chase(actor: &mut MapObject) {
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
        let delta = actor
            .angle
            .unit()
            .angle_to(Angle::from(actor.movedir).unit());
        if delta > FRAC_PI_4 {
            actor.angle += FRAC_PI_4;
        } else if delta < -FRAC_PI_4 {
            actor.angle -= FRAC_PI_4;
        }
    }

    // If already have a target check if proper, else look for it
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj() };
        // Inanimate object, try to find new target
        if target.flags & MapObjFlag::Shootable as u32 == 0 {
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

    if actor.flags & MapObjFlag::Justattacked as u32 != 0 {
        actor.flags &= !(MapObjFlag::Justattacked as u32);
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
    }

    // Missile attack?
    if actor.info.missilestate != StateNum::None {
        let skill = unsafe { (*actor.level).options.skill };
        if skill == Skill::Nightmare || actor.movecount <= 0 {
            // if (gameskill < sk_nightmare && !fastparm && actor->movecount) {
            // goto nomissile;
            // }
            if actor.check_missile_range() {
                actor.flags |= MapObjFlag::Justattacked as u32;
                actor.set_state(actor.info.missilestate);
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
    if actor.movecount < 0 || !actor.do_move() {
        actor.new_chase_dir()
    }

    // make active sound
    if actor.info.activesound != SfxName::None && p_random() < 3 {
        actor.start_sound(actor.info.activesound);
    }
}

/// Stay in this state until a player is sighted.
pub(crate) fn a_look(actor: &mut MapObject) {
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
    if let Some(target) = ss.sector.sound_target() {
        if target.flags & MapObjFlag::Shootable as u32 != 0 {
            actor.target = actor.subsector.sector.sound_target_raw();

            if actor.flags & MapObjFlag::Ambush as u32 != 0 && !actor.check_sight_target(target) {
                return;
            }
        } else if !actor.look_for_players(false) {
            return;
        }
    } else if !actor.look_for_players(false) {
        return;
    }

    if actor.info.seesound != SfxName::None {
        let sound = match actor.info.seesound {
            SfxName::Posit1 | SfxName::Posit2 | SfxName::Posit3 => {
                SfxName::from((SfxName::Posit1 as i32 + p_random() % 3) as u8)
            }
            SfxName::Bgsit1 | SfxName::Bgsit2 => {
                SfxName::from((SfxName::Bgsit1 as i32 + p_random() % 3) as u8)
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
    if let Some(dest) = actor.tracer {
        let dest = unsafe { (*dest).mobj() };
        if let Some(targ) = actor.target_mut() {
            // don't move it if the vile lost sight
            if !targ.check_sight_target(dest) {
                return;
            }

            unsafe { actor.unset_thing_position() };
            actor.xy.x = dest.xy.x + 24.0 * dest.angle.cos();
            actor.xy.y = dest.xy.y + 24.0 * dest.angle.sin();
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
    actor.flags &= !(MapObjFlag::Solid as u32);
    // So change this if corpse objects are meant to be obstacles.
}

pub(crate) fn a_explode(actor: &mut MapObject) {
    actor.radius_attack(128.0);
}

pub(crate) fn a_xscream(actor: &mut MapObject) {
    actor.start_sound(SfxName::Slop);
}

pub(crate) fn a_keendie(actor: &mut MapObject) {
    a_fall(actor);

    let level = unsafe { &mut *actor.level };
    // Check keens are all dead
    let mut dead = true;
    level.thinkers.run_fn_on_things(|thinker| {
        if let &ThinkerData::MapObject(ref mobj) = thinker.data() {
            if !ptr::eq(mobj, actor) && mobj.kind == actor.kind && mobj.health > 0 {
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
        v1: Default::default(),
        v2: Default::default(),
        delta: Default::default(),
        flags: 0,
        special: 0,
        tag: 666,
        bbox: Default::default(),
        slopetype: SlopeType::Horizontal,
        front_sidedef: sidedef,
        back_sidedef: None,
        frontsector: sector,
        backsector: None,
        valid_count: 0,
        sides: [0, 0],
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

    let target_xy = target_thinker.mobj_mut().xy;
    let level = unsafe { &mut *actor.level };
    let m = MapObject::spawn_missile(
        actor,
        target_thinker.mobj_mut(),
        MapObjKind::MT_SPAWNSHOT,
        level,
    );
    m.target = Some(target_thinker);
    m.reactiontime = ((target_xy.y - actor.xy.y) / m.momxy.y) as i32 / m.state.tics;

    actor.start_sound(SfxName::Bospit);
}

pub(crate) fn a_brainpain(actor: &mut MapObject) {
    actor.start_sound(SfxName::Bospn);
}

pub(crate) fn a_brainscream(actor: &mut MapObject) {
    let mut x = actor.xy.x as i32 - 196;
    while x < actor.xy.x as i32 + 320 {
        let y = actor.xy.y - 320.0;
        let z = 128 + p_random();
        let level = unsafe { &mut *actor.level };
        let th = MapObject::spawn_map_object(x as f32, y, z, MapObjKind::MT_ROCKET, level);
        unsafe {
            let th = &mut (*th);
            th.momz = (p_random() as f32 / 64.0).ceil();
            th.set_state(StateNum::BRAINEXPLODE1);
            th.tics -= p_random() & 7;
            if th.tics < 1 {
                th.tics = 1;
            }
        }
        x += 8;
    }
    actor.start_sound(SfxName::Bosdth);
}

pub(crate) fn a_brainexplode(actor: &mut MapObject) {
    let x = actor.xy.x + (p_random() - p_random()) as f32 * 2.0;
    let y = actor.xy.y;
    let z = 128 + p_random();
    let level = unsafe { &mut *actor.level };
    let th = MapObject::spawn_map_object(x, y, z, MapObjKind::MT_ROCKET, level);
    unsafe {
        let th = &mut (*th);
        th.momz = (p_random() as f32 / 64.0).ceil();
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
        let xy = target.xy;
        let fog = unsafe {
            &mut *MapObject::spawn_map_object(
                xy.x,
                xy.y,
                target.z as i32,
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
            unsafe { &mut *MapObject::spawn_map_object(xy.x, xy.y, target.z as i32, t, level) };
        if new_critter.look_for_players(true) {
            new_critter.set_state(new_critter.info.seestate);
        }
        teleport_move(xy, new_critter, level);
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
    if obj.flags & MapObjFlag::Corpse as u32 != MapObjFlag::Corpse as u32 {
        return true; // not a monster
    }

    if obj.tics != -1 {
        return true; // not lying still yet
    }

    if obj.info.raisestate == StateNum::None {
        return true; // monster doesn't have a raise state
    }

    let max_dist = obj.radius + actor.radius;
    let try_dist = actor.xy + actor.info.speed * Angle::from(actor.movedir).unit();
    if obj.xy.distance(try_dist) > max_dist {
        return true;
    }

    obj.momxy.x = 0.0;
    obj.momxy.y = 0.0;
    let old_height = obj.height;
    obj.height = obj.info.height;
    let mut ctrl = SubSectorMinMax::default();
    let check = obj.p_check_position(obj.xy, &mut ctrl);
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
                obj.height *= 2.0;
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
        let fog = MapObject::spawn_map_object(
            targ.xy.x,
            targ.xy.y,
            targ.z as i32,
            MapObjKind::MT_FIRE,
            level,
        );
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
        targ.p_take_damage(Some(actor), None, true, 20);
        targ.momz = 1000.0 / targ.info.mass as f32;

        if let Some(fire) = actor.tracer {
            let fire = unsafe { (*fire).mobj_mut() };
            fire.xy.x = targ.xy.x - 24.0 * actor.angle.cos();
            fire.xy.y = targ.xy.y - 24.0 * actor.angle.sin();
            fire.radius_attack(70.0);
        }
    }
}

pub(crate) fn a_posattack(actor: &mut MapObject) {
    if actor.target.is_none() {
        return;
    }

    a_facetarget(actor);
    let mut bsp_trace = actor.get_shoot_bsp_trace(MISSILERANGE);
    let slope = actor.aim_line_attack(MISSILERANGE, &mut bsp_trace);

    actor.start_sound(SfxName::Pistol);

    let mut angle = actor.angle;
    angle += (((p_random() - p_random()) >> 4) as f32).to_radians();
    let damage = ((p_random() % 5) + 1) * 3;
    actor.line_attack(damage as f32, MISSILERANGE, angle, slope, &mut bsp_trace);
}

pub(crate) fn a_sposattack(actor: &mut MapObject) {
    if actor.target.is_none() {
        return;
    }

    a_facetarget(actor);
    let mut bsp_trace = actor.get_shoot_bsp_trace(MISSILERANGE);
    let slope = actor.aim_line_attack(MISSILERANGE, &mut bsp_trace);

    actor.start_sound(SfxName::Shotgn);

    let mut angle;
    for _ in 0..3 {
        angle = actor.angle + (((p_random() - p_random()) >> 4) as f32).to_radians();
        let damage = ((p_random() % 5) + 1) * 3;
        actor.line_attack(
            damage as f32,
            MISSILERANGE,
            angle,
            slope.clone(),
            &mut bsp_trace,
        );
    }
}

pub(crate) fn a_cposattack(actor: &mut MapObject) {
    if actor.target.is_none() {
        return;
    }

    a_facetarget(actor);
    let mut bsp_trace = actor.get_shoot_bsp_trace(MISSILERANGE);
    let slope = actor.aim_line_attack(MISSILERANGE, &mut bsp_trace);

    actor.start_sound(SfxName::Shotgn);

    let mut angle = actor.angle;
    angle += (((p_random() - p_random()) >> 4) as f32).to_radians();
    let damage = ((p_random() % 5) + 1) * 3;
    actor.line_attack(damage as f32, MISSILERANGE, angle, slope, &mut bsp_trace);
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
        actor.flags |= MapObjFlag::Skullfly as u32;
        actor.start_sound(actor.info.attacksound);

        actor.angle = point_to_angle_2(target.xy, actor.xy);
        actor.momxy = actor.angle.unit() * SKULLSPEED;

        let mut dist = actor.xy.distance(target.xy) / SKULLSPEED;
        if dist < 1.0 {
            dist = 1.0;
        }

        actor.momz = (target.z + (target.height / 2.0) - actor.z) / dist;
    }
}

pub(crate) fn a_headattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj_mut() };
        a_facetarget(actor);

        if actor.check_melee_range() {
            actor.start_sound(SfxName::Claw);
            let damage = ((p_random() % 8) + 1) * 10;
            target.p_take_damage(Some(actor), None, true, damage);
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
            target.p_take_damage(Some(actor), None, true, damage);
        }
    }
}

pub(crate) fn a_bruisattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj_mut() };
        a_facetarget(actor);

        if actor.check_melee_range() {
            let damage = ((p_random() % 6) + 1) * 10;
            target.p_take_damage(Some(actor), None, true, damage);
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
            target.p_take_damage(Some(actor), None, true, damage);
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

fn a_painshootskull(actor: &mut MapObject, angle: Angle) {
    a_facetarget(actor);
    // TODO: limit amount of skulls
    //
    let mut d = angle.unit();
    d += 4.0 + 3.0 * (actor.radius + MOBJINFO[MapObjKind::MT_SKULL as usize].radius) / 2.0;

    let level = unsafe { &mut *actor.level };
    unsafe {
        let skull = &mut (*MapObject::spawn_map_object(
            actor.xy.x + d.x,
            actor.xy.y + d.y,
            actor.z as i32 + 8,
            MapObjKind::MT_SKULL,
            level,
        ));
        let mut ctrl = SubSectorMinMax::default();
        if !skull.p_try_move(skull.xy.x, skull.xy.y, &mut ctrl) {
            skull.p_take_damage(None, None, false, 10000);
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
    a_painshootskull(actor, actor.angle + FRAC_PI_2);
    a_painshootskull(actor, actor.angle + PI);
    a_painshootskull(actor, actor.angle + PI + FRAC_PI_2);
}

const FAT_SPREAD: f32 = FRAC_PI_2 / 8.0;

pub(crate) fn a_fatattack1(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let level = unsafe { &mut *actor.level };
        let target = unsafe { (*target).mobj_mut() };

        a_facetarget(actor);
        actor.angle += FAT_SPREAD;
        // 1 away
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_FATSHOT, level);
        let an = missile.angle;
        missile.momxy.x = missile.info.speed * an.cos();
        missile.momxy.y = missile.info.speed * an.sin();

        // 2 away
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_FATSHOT, level);
        actor.angle += FAT_SPREAD;
        let an = missile.angle;
        missile.momxy.x = missile.info.speed * an.cos();
        missile.momxy.y = missile.info.speed * an.sin();
    }
}
pub(crate) fn a_fatattack2(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let level = unsafe { &mut *actor.level };
        let target = unsafe { (*target).mobj_mut() };

        a_facetarget(actor);
        actor.angle -= FAT_SPREAD;
        // 1 away
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_FATSHOT, level);
        let an = missile.angle;
        missile.momxy.x = missile.info.speed * an.cos();
        missile.momxy.y = missile.info.speed * an.sin();

        // 2 away
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_FATSHOT, level);
        actor.angle -= FAT_SPREAD * 2.0;
        let an = missile.angle;
        missile.momxy.x = missile.info.speed * an.cos();
        missile.momxy.y = missile.info.speed * an.sin();
    }
}
pub(crate) fn a_fatattack3(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let level = unsafe { &mut *actor.level };
        let target = unsafe { (*target).mobj_mut() };

        a_facetarget(actor);
        actor.angle -= FAT_SPREAD / 2.0;
        // 1 away
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_FATSHOT, level);
        let an = missile.angle;
        missile.momxy.x = missile.info.speed * an.cos();
        missile.momxy.y = missile.info.speed * an.sin();

        // 2 away
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_FATSHOT, level);
        actor.angle += FAT_SPREAD / 2.0;
        let an = missile.angle;
        missile.momxy.x = missile.info.speed * an.cos();
        missile.momxy.y = missile.info.speed * an.sin();
    }
}

pub(crate) fn a_fatraise(actor: &mut MapObject) {
    a_facetarget(actor);
    actor.start_sound(SfxName::Manatk);
}

pub(crate) fn a_spidrefire(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        a_facetarget(actor);
        if p_random() < 10 {
            return;
        }

        let target = unsafe { (*target).mobj_mut() };
        if target.health <= 0 || !actor.check_sight_target(target) {
            actor.set_state(actor.info.seestate);
        }
    }
    actor.set_state(actor.info.seestate);
}

pub(crate) fn a_bossdeath(actor: &mut MapObject) {
    let level = unsafe { &mut *actor.level };
    let map = level.options.map;
    let episode = level.options.episode;
    let mode = level.game_mode;
    let mt = actor.kind;

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
        // There needs to be at least one player alive
        for (i, p) in level.players().iter().enumerate() {
            if p.status.health > 0 {
                break;
            }
            if i == MAXPLAYERS - 1 {
                return;
            }
        }
    }

    // Check bosses are all dead
    let mut dead = true;
    level.thinkers.run_fn_on_things(|thinker| {
        if let &ThinkerData::MapObject(ref mobj) = thinker.data() {
            if !ptr::eq(mobj, actor) && mobj.kind == actor.kind && mobj.health > 0 {
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
        v1: Default::default(),
        v2: Default::default(),
        delta: Default::default(),
        flags: 0,
        special: 0,
        tag: 666,
        bbox: Default::default(),
        slopetype: SlopeType::Horizontal,
        front_sidedef: sidedef,
        back_sidedef: None,
        frontsector: sector,
        backsector: None,
        valid_count: 0,
        sides: [0, 0],
    };

    if mode == GameMode::Commercial && map == 7 {
        if actor.kind == MapObjKind::MT_FATSO {
            junk.tag = 666;
            ev_do_floor(MapPtr::new(&mut junk), FloorKind::LowerFloorToLowest, level);
            return;
        }
        if actor.kind == MapObjKind::MT_BABY {
            junk.tag = 667;
            ev_do_floor(MapPtr::new(&mut junk), FloorKind::RaiseToTexture, level);
            return;
        }
    } else if episode == 1 {
        junk.tag = 666;
        ev_do_floor(MapPtr::new(&mut junk), FloorKind::LowerFloorToLowest, level);
        return;
    } else if episode == 4 {
        if map == 6 {
            junk.tag = 666;
            ev_do_door(MapPtr::new(&mut junk), DoorKind::BlazeOpen, level);
            return;
        } else if map == 8 {
            junk.tag = 666;
            ev_do_floor(MapPtr::new(&mut junk), FloorKind::LowerFloorToLowest, level);
            return;
        }
    }

    level.do_completed();
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
            target.p_take_damage(Some(actor), None, true, damage);
        }
    }
}

pub(crate) fn a_skelmissile(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj_mut() };
        a_facetarget(actor);

        let level = unsafe { &mut *actor.level };
        actor.z += 16.0;
        let missile = MapObject::spawn_missile(actor, target, MapObjKind::MT_TRACER, level);
        actor.z -= 16.0;

        missile.xy += missile.momxy;
        missile.tracer = actor.target;
    }
}

/// Skelly missile that tracks the player/target
pub(crate) fn a_tracer(actor: &mut MapObject) {
    let level = unsafe { &mut *actor.level };
    // spawn a puff of smoke behind the rocket
    MapObject::spawn_puff(actor.xy.x, actor.xy.y, actor.z as i32, 0.0, level);
    let thing = MapObject::spawn_map_object(
        actor.xy.x,
        actor.xy.y,
        actor.z as i32,
        MapObjKind::MT_SMOKE,
        level,
    );
    let smoke = unsafe { &mut *thing };
    smoke.momz = 1.0;
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

        let an = point_to_angle_2(dest.mobj().xy, actor.xy);
        actor.momxy.x = actor.info.speed * an.cos();
        actor.momxy.y = actor.info.speed * an.sin();

        let mut dist = actor.xy.distance(dest.mobj().xy) / actor.info.speed;
        if dist < 1.0 {
            dist = 1.0;
        }
        let slope = (dest.mobj().z + 40.0 - actor.z) / dist;
        if slope < actor.momz {
            actor.momz -= 1.0 / 8.0;
        } else {
            actor.momz += 1.0 / 8.0;
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
