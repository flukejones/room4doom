//! Doom source name `p_enemy`
//!
//! ENEMY THINKING
//! Enemies are allways spawned
//! with targetplayer = -1, threshold = 0
//! Most monsters are spawned unaware of all players,
//! but some can be made preaware
//!

use std::{f32::consts::FRAC_PI_4, ptr};

use log::error;
use sound_traits::SfxEnum;

use crate::{
    doom_def::{MISSILERANGE, SKULLSPEED},
    info::StateNum,
    play::{mobj::DirType, utilities::p_random},
    Angle, DPtr, LineDefFlags, MapObjectType, Sector, Skill,
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
    sector.sound_target = Some(target);

    for line in sector.lines.iter() {
        if line.flags & LineDefFlags::TwoSided as u32 == 0 {
            continue;
        }

        let line_opening = PortalZ::new(line);
        if line_opening.range <= 0.0 {
            continue; // A door, and it's closed
        }

        let other = if ptr::eq(line.front_sidedef.sector.as_ptr(), sector.as_ptr()) {
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

use super::{
    mobj::{MapObject, MapObjectFlag},
    utilities::{point_to_angle_2, PortalZ},
};

/// A_FaceTarget
pub fn a_facetarget(actor: &mut MapObject) {
    actor.flags &= !(MapObjectFlag::Ambush as u32);

    if let Some(target) = actor.target {
        unsafe {
            let angle = point_to_angle_2((*target).xy, actor.xy);
            actor.angle = angle;

            if (*target).flags & MapObjectFlag::Shadow as u32 == MapObjectFlag::Shadow as u32 {
                actor.angle += (((p_random() - p_random()) >> 4) as f32).to_radians();
            }
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
            unsafe {
                if (*target).health <= 0 {
                    actor.threshold = 0;
                } else {
                    actor.threshold -= 1;
                }
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
        unsafe {
            // Inanimate object, try to find new target
            if (*target).flags & MapObjectFlag::Shootable as u32 == 0 {
                if actor.look_for_players(true) {
                    return; // Found a new target
                }
                actor.set_state(actor.info.spawnstate);
                return;
            }
        }
    } else {
        if actor.look_for_players(true) {
            return; // Found a new target
        }
        actor.set_state(actor.info.spawnstate);
        return;
    }

    if actor.flags & MapObjectFlag::JustAttacked as u32 != 0 {
        actor.flags &= !(MapObjectFlag::JustAttacked as u32);
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
                actor.flags |= MapObjectFlag::JustAttacked as u32;
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
        if let Some(target) = (*actor.subsector).sector.sound_target {
            let target = &*target;
            if target.flags & MapObjectFlag::Shootable as u32 != 0 {
                actor.target = (*actor.subsector).sector.sound_target;

                if actor.flags & MapObjectFlag::Ambush as u32 != 0
                    && !actor.check_sight_target(target)
                    && !actor.look_for_players(false)
                {
                    return;
                }
            }
        } else if !actor.look_for_players(false) {
            return;
        }
    }

    if actor.info.seesound != SfxEnum::None {
        let sound = match actor.info.seesound {
            SfxEnum::posit1 | SfxEnum::posit2 | SfxEnum::posit3 => {
                SfxEnum::from((SfxEnum::posit1 as i32 + p_random() % 3) as u8)
            }
            SfxEnum::bgsit1 | SfxEnum::bgsit2 => {
                SfxEnum::from((SfxEnum::bgsit1 as i32 + p_random() % 3) as u8)
            }
            _ => actor.info.seesound,
        };

        if actor.kind == MapObjectType::MT_SPIDER || actor.kind == MapObjectType::MT_CYBORG {
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
        SfxEnum::podth1 | SfxEnum::podth2 | SfxEnum::podth3 => {
            SfxEnum::from(SfxEnum::podth1 as u8 + (p_random() % 3) as u8)
        }
        SfxEnum::bgdth1 | SfxEnum::bgdth2 => {
            SfxEnum::from(SfxEnum::bgdth1 as u8 + (p_random() % 2) as u8)
        }
        _ => SfxEnum::from(actor.info.deathsound as u8),
    };

    // Check for bosses.
    if matches!(
        actor.kind,
        MapObjectType::MT_SPIDER | MapObjectType::MT_CYBORG
    ) {
        // full volume
        // TODO: start_sound("a_scream", None, sound);
    } else {
        actor.start_sound(sound);
    }
}

pub fn a_fall(actor: &mut MapObject) {
    // actor is on ground, it can be walked over
    actor.flags &= !(MapObjectFlag::Solid as u32);
    // So change this if corpse objects are meant to be obstacles.
}

pub fn a_explode(actor: &mut MapObject) {
    actor.radius_attack(128.0);
}

pub fn a_xscream(actor: &mut MapObject) {
    actor.start_sound(SfxEnum::slop);
}

pub fn a_keendie(_actor: &mut MapObject) {
    error!("a_keendie not implemented");
}

pub fn a_hoof(actor: &mut MapObject) {
    error!("a_hoof not implemented");
}

pub fn a_metal(actor: &mut MapObject) {
    error!("a_metal not implemented");
}

pub fn a_babymetal(actor: &mut MapObject) {
    error!("a_babymetal not implemented");
}

pub fn a_brainawake(actor: &mut MapObject) {
    error!("a_brainawake not implemented");
}

pub fn a_braindie(actor: &mut MapObject) {
    error!("a_braindie not implemented");
}

pub fn a_brainspit(actor: &mut MapObject) {
    error!("a_brainspit not implemented");
}

pub fn a_brainpain(actor: &mut MapObject) {
    actor.start_sound(SfxEnum::bospn);
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
    actor.start_sound(SfxEnum::boscub);
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

    actor.start_sound(SfxEnum::pistol);

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

    actor.start_sound(SfxEnum::pistol);

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
    error!("a_cposattack not implemented");
}

pub fn a_bspiattack(actor: &mut MapObject) {
    error!("a_bspiattack not implemented");
}

pub fn a_skullattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        let target = unsafe { &*target };

        a_facetarget(actor);
        actor.flags |= MapObjectFlag::SkullFly as u32;
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
    error!("a_headattack not implemented");
}

pub fn a_sargattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        a_facetarget(actor);
        if actor.check_melee_range() {
            let damage = ((p_random() % 10) + 1) * 4;
            unsafe {
                (*target).p_take_damage(Some(actor), None, true, damage);
            }
        }
    }
}

pub fn a_bruisattack(actor: &mut MapObject) {
    error!("a_bruisattack not implemented");
}

pub fn a_cposrefire(actor: &mut MapObject) {
    error!("a_cposrefire not implemented");
}

pub fn a_cyberattack(actor: &mut MapObject) {
    error!("a_cyberattack not implemented");
}

pub fn a_troopattack(actor: &mut MapObject) {
    if let Some(target) = actor.target {
        a_facetarget(actor);

        let target = unsafe { &mut *target };

        if actor.check_melee_range() {
            actor.start_sound(SfxEnum::claw);
            let damage = ((p_random() % 8) + 1) * 3;
            target.p_take_damage(Some(actor), None, true, damage);
            return;
        }

        let level = unsafe { &mut *actor.level };
        MapObject::spawn_missile(actor, target, MapObjectType::MT_TROOPSHOT, level);
    }
}

pub fn a_pain(actor: &mut MapObject) {
    if actor.info.painsound != SfxEnum::None {
        actor.start_sound(actor.info.painsound);
    }
}

pub fn a_painattack(actor: &mut MapObject) {
    error!("a_painattack not implemented");
}

pub fn a_paindie(actor: &mut MapObject) {
    error!("a_paindie not implemented");
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
    error!("a_fatraise not implemented");
}

pub fn a_spidrefire(actor: &mut MapObject) {
    error!("a_spidrefire not implemented");
}

pub fn a_bossdeath(actor: &mut MapObject) {
    error!("a_bossdeath not implemented");
}

pub fn a_skelwhoosh(actor: &mut MapObject) {
    error!("a_skelwhoosh not implemented");
}

pub fn a_skelfist(actor: &mut MapObject) {
    error!("a_skelfist not implemented");
}

pub fn a_skelmissile(actor: &mut MapObject) {
    error!("a_skelmissile not implemented");
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
    error!("a_playerscream not implemented");
}
