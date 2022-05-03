//! ENEMY THINKING
//! Enemies are always spawned with targetplayer = -1, threshold = 0.
//!
//! Most monsters are spawned unaware of all players, but some can be made aware
//! on spawn.
//!
//! Doom source name `p_enemy`

use std::{f32::consts::FRAC_PI_4, ptr, ptr::null_mut};

use log::error;
use sound_traits::SfxEnum;

use crate::{
    doom_def::{MISSILERANGE, SKULLSPEED},
    env::{
        doors::{ev_do_door, DoorKind},
        floor::{ev_do_floor, FloorKind},
    },
    info::StateNum,
    level::map_defs::{LineDef, SlopeType},
    obj::{DirType, MapObjFlag, MapObject},
    thinker::{Thinker, ThinkerData},
    utilities::{p_random, point_to_angle_2, PortalZ},
    Angle, DPtr, GameMode, LineDefFlags, MapObjKind, Sector, Skill, MAXPLAYERS,
};

/// This was only ever called with the player as the target, so it never follows
/// the original comment stating that if a monster yells it alerts surrounding monsters
pub(crate) fn noise_alert(target: &mut MapObject) {
    let vc = unsafe {
        (*target.level).valid_count += 1;
        (*target.level).valid_count
    };
    let sect = unsafe { (*target.subsector).sector.clone() };
    sound_flood(sect, vc, 0, target);
}

fn sound_flood(
    mut sector: DPtr<Sector>,
    valid_count: usize,
    sound_blocks: i32,
    target: &mut MapObject,
) {
    if sector.validcount == valid_count && sector.soundtraversed <= sound_blocks + 1 {
        return; // Done with this sector, it's flooded
    }

    sector.validcount = valid_count;
    sector.soundtraversed = sound_blocks + 1;
    sector.set_sound_target(target.thinker);

    for line in sector.lines.iter() {
        if line.flags & LineDefFlags::TwoSided as u32 == 0 {
            continue;
        }

        let line_opening = PortalZ::new(line);
        if line_opening.range <= 0.0 {
            continue; // A door, and it's closed
        }

        let other = if ptr::eq(line.front_sidedef.sector.as_ref(), sector.as_ref()) {
            line.back_sidedef.as_ref().unwrap().sector.clone()
        } else {
            line.front_sidedef.sector.clone()
        };

        if line.flags & LineDefFlags::BlockSound as u32 != 0 {
            if sound_blocks == 0 {
                sound_flood(other, valid_count, 1, target);
            }
        } else {
            sound_flood(other, valid_count, sound_blocks, target);
        }
    }
}

/// A_FaceTarget
pub fn a_facetarget(actor: &mut MapObject) {
    actor.flags &= !(MapObjFlag::Ambush as u32);

    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj() };

        let angle = point_to_angle_2(target.xy, actor.xy);
        actor.angle = angle;

        if target.flags & MapObjFlag::Shadow as u32 == MapObjFlag::Shadow as u32 {
            actor.angle += (((p_random() - p_random()) >> 4) as f32).to_radians();
        }
    }
}

/// Actor has a melee attack,
/// so it tries to close as fast as possible
pub fn a_chase(actor: &mut MapObject) {
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

    if actor.movedir < DirType::NoDir {
        let delta = actor.angle.rad() - Angle::from(actor.movedir).rad();
        if delta > FRAC_PI_4 {
            actor.angle -= FRAC_PI_4;
        } else if delta < -FRAC_PI_4 {
            actor.angle += FRAC_PI_4;
        }
    }

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
        actor.new_chase_dir();
        return;
    }

    // Melee attack?
    if actor.info.meleestate != StateNum::S_NULL && actor.check_melee_range() {
        if actor.info.attacksound != SfxEnum::None {
            actor.start_sound(actor.info.attacksound);
        }
        actor.set_state(actor.info.meleestate);
    }

    // Missile attack?
    if actor.info.missilestate != StateNum::S_NULL {
        let skill = unsafe { (*actor.level).game_skill };
        if skill >= Skill::Nightmare || actor.movecount <= 0 {
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
    if actor.info.activesound != SfxEnum::None && p_random() < 3 {
        actor.start_sound(actor.info.activesound);
    }
}

/// Stay in this state until a player is sighted.
pub fn a_look(actor: &mut MapObject) {
    actor.threshold = 0;
    // TODO: any shot will wake up
    unsafe {
        // if let Some(target) = actor.target {
        //     let target = &*target;
        //     if target.health <= 0 {
        //         actor.set_state(actor.info.spawnstate);
        //         return;
        //     }
        // }

        if let Some(target) = (*actor.subsector).sector.sound_target() {
            if target.flags & MapObjFlag::Shootable as u32 != 0 {
                actor.target = (*actor.subsector).sector.sound_target_raw();

                if actor.flags & MapObjFlag::Ambush as u32 != 0
                    && !actor.check_sight_target(target)
                    && !actor.look_for_players(false)
                {
                    return;
                }
            } else if !actor.look_for_players(false) {
                return;
            }
        } else if !actor.look_for_players(false) {
            return;
        }
    }

    if actor.info.seesound != SfxEnum::None {
        let sound = match actor.info.seesound {
            SfxEnum::Posit1 | SfxEnum::Posit2 | SfxEnum::Posit3 => {
                SfxEnum::from((SfxEnum::Posit1 as i32 + p_random() % 3) as u8)
            }
            SfxEnum::Bgsit1 | SfxEnum::Bgsit2 => {
                SfxEnum::from((SfxEnum::Bgsit1 as i32 + p_random() % 3) as u8)
            }
            _ => actor.info.seesound,
        };

        if actor.kind == MapObjKind::MT_SPIDER || actor.kind == MapObjKind::MT_CYBORG {
            // TODO: FULL VOLUME!
            actor.start_sound(sound);
        } else {
            actor.start_sound(sound);
        }
    }

    actor.set_state(actor.info.seestate);
}

pub fn a_fire(_actor: &mut MapObject) {
    error!("a_fire not implemented");
    // mobj_t *dest;
    // mobj_t *target;
    // unsigned an;
    //
    // dest = actor->tracer;
    // if (!dest)
    // return;
    //
    // target = P_SubstNullMobj(actor->target);
    //
    // // don't move it if the vile lost sight
    // if (!P_CheckSight(target, dest))
    // return;
    //
    // an = dest->angle >> ANGLETOFINESHIFT;
    //
    // P_UnsetThingPosition(actor);
    // actor->x = dest->x + FixedMul(24 * FRACUNIT, finecosine[an]);
    // actor->y = dest->y + FixedMul(24 * FRACUNIT, finesine[an]);
    // actor->z = dest->z;
    // P_SetThingPosition(actor);
}

pub fn a_scream(actor: &mut MapObject) {
    let sound = match actor.info.deathsound {
        SfxEnum::None => return,
        SfxEnum::Podth1 | SfxEnum::Podth2 | SfxEnum::Podth3 => {
            SfxEnum::from(SfxEnum::Podth1 as u8 + (p_random() % 3) as u8)
        }
        SfxEnum::Bgdth1 | SfxEnum::Bgdth2 => {
            SfxEnum::from(SfxEnum::Bgdth1 as u8 + (p_random() % 2) as u8)
        }
        _ => SfxEnum::from(actor.info.deathsound as u8),
    };

    // Check for bosses.
    if matches!(actor.kind, MapObjKind::MT_SPIDER | MapObjKind::MT_CYBORG) {
        // full volume
        // TODO: start_sound("a_scream", None, sound);
    } else {
        actor.start_sound(sound);
    }
}

pub fn a_fall(actor: &mut MapObject) {
    // actor is on ground, it can be walked over
    actor.flags &= !(MapObjFlag::Solid as u32);
    // So change this if corpse objects are meant to be obstacles.
}

pub fn a_explode(actor: &mut MapObject) {
    actor.radius_attack(128.0);
}

pub fn a_xscream(actor: &mut MapObject) {
    actor.start_sound(SfxEnum::Slop);
}

pub fn a_keendie(_actor: &mut MapObject) {
    error!("a_keendie not implemented");
}

pub fn a_hoof(actor: &mut MapObject) {
    actor.start_sound(SfxEnum::Hoof);
    a_chase(actor);
}

pub fn a_metal(actor: &mut MapObject) {
    actor.start_sound(SfxEnum::Metal);
    a_chase(actor);
}

pub fn a_babymetal(actor: &mut MapObject) {
    actor.start_sound(SfxEnum::Bspwlk);
    a_chase(actor);
}

pub fn a_brainawake(actor: &mut MapObject) {
    error!("a_brainawake not implemented");
}

pub fn a_braindie(actor: &mut MapObject) {
    actor.level_mut().do_exit_level();
}

pub fn a_brainspit(actor: &mut MapObject) {
    error!("a_brainspit not implemented");
}

pub fn a_brainpain(actor: &mut MapObject) {
    actor.start_sound(SfxEnum::Bospn);
}

pub fn a_brainscream(actor: &mut MapObject) {
    error!("a_brainscream not implemented");
}

pub fn a_brainexplode(actor: &mut MapObject) {
    error!("a_brainexplode not implemented");
}

pub fn a_spawnfly(actor: &mut MapObject) {
    error!("a_spawnfly not implemented");
}

pub fn a_spawnsound(actor: &mut MapObject) {
    actor.start_sound(SfxEnum::Boscub);
    a_spawnfly(actor);
}

pub fn a_vilestart(actor: &mut MapObject) {
    error!("a_vilestart not implemented");
}

pub fn a_vilechase(actor: &mut MapObject) {
    error!("a_vilechase not implemented");
}

pub fn a_viletarget(actor: &mut MapObject) {
    error!("a_viletarget not implemented");
}

pub fn a_vileattack(actor: &mut MapObject) {
    error!("a_vileattack not implemented");
}

pub fn a_posattack(actor: &mut MapObject) {
    if actor.target.is_none() {
        return;
    }

    a_facetarget(actor);
    let mut bsp_trace = actor.get_shoot_bsp_trace(MISSILERANGE);
    let slope = actor.aim_line_attack(MISSILERANGE, &mut bsp_trace);

    actor.start_sound(SfxEnum::Pistol);

    let mut angle = actor.angle;
    angle += (((p_random() - p_random()) >> 4) as f32).to_radians();
    let damage = ((p_random() % 5) + 1) * 3;
    actor.line_attack(damage as f32, MISSILERANGE, angle, slope, &mut bsp_trace);
}

pub fn a_sposattack(actor: &mut MapObject) {
    if actor.target.is_none() {
        return;
    }

    a_facetarget(actor);
    let mut bsp_trace = actor.get_shoot_bsp_trace(MISSILERANGE);
    let slope = actor.aim_line_attack(MISSILERANGE, &mut bsp_trace);

    actor.start_sound(SfxEnum::Shotgn);

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

pub fn a_cposattack(actor: &mut MapObject) {
    if actor.target.is_none() {
        return;
    }

    a_facetarget(actor);
    let mut bsp_trace = actor.get_shoot_bsp_trace(MISSILERANGE);
    let slope = actor.aim_line_attack(MISSILERANGE, &mut bsp_trace);

    actor.start_sound(SfxEnum::Shotgn);

    let mut angle = actor.angle;
    angle += (((p_random() - p_random()) >> 4) as f32).to_radians();
    let damage = ((p_random() % 5) + 1) * 3;
    actor.line_attack(damage as f32, MISSILERANGE, angle, slope, &mut bsp_trace);
}

pub fn a_bspiattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj_mut() };
        a_facetarget(actor);

        let level = unsafe { &mut *actor.level };
        MapObject::spawn_missile(actor, target, MapObjKind::MT_ARACHPLAZ, level);
    }
}

pub fn a_skullattack(actor: &mut MapObject) {
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

pub fn a_headattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj_mut() };
        a_facetarget(actor);

        if actor.check_melee_range() {
            actor.start_sound(SfxEnum::Claw);
            let damage = ((p_random() % 8) + 1) * 10;
            target.p_take_damage(Some(actor), None, true, damage);
            return;
        }

        let level = unsafe { &mut *actor.level };
        MapObject::spawn_missile(actor, target, MapObjKind::MT_BRUISERSHOT, level);
    }
}

pub fn a_sargattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj_mut() };

        a_facetarget(actor);
        if actor.check_melee_range() {
            let damage = ((p_random() % 10) + 1) * 4;
            target.p_take_damage(Some(actor), None, true, damage);
        }
    }
}

pub fn a_bruisattack(actor: &mut MapObject) {
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

pub fn a_cposrefire(actor: &mut MapObject) {
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
    actor.set_state(actor.info.seestate);
}

pub fn a_cyberattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        a_facetarget(actor);
        let target = unsafe { (*target).mobj_mut() };

        let level = unsafe { &mut *actor.level };
        MapObject::spawn_missile(actor, target, MapObjKind::MT_ROCKET, level);
    }
}

pub fn a_troopattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        a_facetarget(actor);

        let target = unsafe { (*target).mobj_mut() };

        if actor.check_melee_range() {
            actor.start_sound(SfxEnum::Claw);
            let damage = ((p_random() % 8) + 1) * 3;
            target.p_take_damage(Some(actor), None, true, damage);
            return;
        }

        let level = unsafe { &mut *actor.level };
        MapObject::spawn_missile(actor, target, MapObjKind::MT_TROOPSHOT, level);
    }
}

pub fn a_pain(actor: &mut MapObject) {
    if actor.info.painsound != SfxEnum::None {
        actor.start_sound(actor.info.painsound);
    }
}

pub fn a_painattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        a_facetarget(actor);
        let _target = unsafe { (*target).mobj_mut() };
        error!("A_PainShootSkull not implemented");
    }
}

pub fn a_paindie(actor: &mut MapObject) {
    error!("a_paindie not implemented");
    // A_Fall(actor);
    // A_PainShootSkull(actor, actor->angle + ANG90);
    // A_PainShootSkull(actor, actor->angle + ANG180);
    // A_PainShootSkull(actor, actor->angle + ANG270);
}

pub fn a_fatattack1(actor: &mut MapObject) {
    error!("a_fatattack1 not implemented");
}
pub fn a_fatattack2(actor: &mut MapObject) {
    error!("a_fatattack2 not implemented");
}
pub fn a_fatattack3(actor: &mut MapObject) {
    error!("a_fatattack3 not implemented");
}

pub fn a_fatraise(actor: &mut MapObject) {
    a_facetarget(actor);
    actor.start_sound(SfxEnum::Manatk);
}

pub fn a_spidrefire(actor: &mut MapObject) {
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

pub fn a_bossdeath(actor: &mut MapObject) {
    let level = unsafe { &mut *actor.level };
    let map = level.game_map;
    let episode = level.episode;
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
            if p.health > 0 {
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
        if let ThinkerData::MapObject(ref mobj) = thinker.data() {
            if !ptr::eq(mobj, actor) && mobj.kind == actor.kind && mobj.health > 0 {
                dead = false;
            }
        }
        true
    });
    if !dead {
        return;
    };

    let mut junk = LineDef {
        v1: Default::default(),
        v2: Default::default(),
        delta: Default::default(),
        flags: 0,
        special: 0,
        tag: 0,
        bbox: Default::default(),
        slopetype: SlopeType::Horizontal,
        front_sidedef: DPtr { inner: null_mut() },
        back_sidedef: None,
        frontsector: DPtr { inner: null_mut() },
        backsector: None,
        valid_count: 0,
    };
    if mode == GameMode::Commercial && map == 7 {
        if actor.kind == MapObjKind::MT_FATSO {
            junk.tag = 666;
            ev_do_floor(DPtr::new(&mut junk), FloorKind::LowerFloorToLowest, level);
            return;
        }
        if actor.kind == MapObjKind::MT_BABY {
            junk.tag = 667;
            ev_do_floor(DPtr::new(&mut junk), FloorKind::RaiseToTexture, level);
            return;
        }
    } else if episode == 1 {
        junk.tag = 666;
        ev_do_floor(DPtr::new(&mut junk), FloorKind::LowerFloorToLowest, level);
        return;
    } else if episode == 4 {
        if map == 6 {
            junk.tag = 666;
            ev_do_door(DPtr::new(&mut junk), DoorKind::BlazeOpen, level);
            return;
        } else if map == 8 {
            junk.tag = 666;
            ev_do_floor(DPtr::new(&mut junk), FloorKind::LowerFloorToLowest, level);
            return;
        }
    }

    level.do_completed();
}

pub fn a_skelwhoosh(actor: &mut MapObject) {
    if actor.target.is_some() {
        a_facetarget(actor);
        actor.start_sound(SfxEnum::Skeswg);
    }
}

pub fn a_skelfist(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        a_facetarget(actor);
        let target = unsafe { (*target).mobj_mut() };

        if actor.check_melee_range() {
            actor.start_sound(SfxEnum::Skepch);
            let damage = ((p_random() % 10) + 1) * 6;
            target.p_take_damage(Some(actor), None, true, damage);
        }
    }
}

pub fn a_skelmissile(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { (*target).mobj_mut() };
        a_facetarget(actor);

        let level = unsafe { &mut *actor.level };
        actor.z += 16.0;
        MapObject::spawn_missile(actor, target, MapObjKind::MT_TRACER, level);
        actor.z -= 16.0;

        // TODO: mo->x += mo->momx;
        //  mo->y += mo->momy;
        //  mo->tracer = actor->target;
    }
}

pub fn a_tracer(actor: &mut MapObject) {
    error!("a_tracer not implemented");
}

pub fn a_startfire(actor: &mut MapObject) {
    error!("a_startfire not implemented");
}

pub fn a_firecrackle(actor: &mut MapObject) {
    error!("a_firecrackle not implemented");
}

pub fn a_playerscream(actor: &mut MapObject) {
    let mut sound = SfxEnum::Pldeth;

    if actor.level().game_mode == GameMode::Commercial && actor.health < -50 {
        // IF THE PLAYER DIES LESS THAN -50% WITHOUT GIBBING
        sound = SfxEnum::Pdiehi;
    }

    actor.start_sound(sound);
}