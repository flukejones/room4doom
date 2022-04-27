//! Doom source name `p_enemy`

use log::error;
use sound_traits::SfxEnum;

use crate::{doom_def::MISSILERANGE, info::StateNum, play::utilities::p_random, MapObjectType};

use super::{
    mobj::{MapObject, MapObjectFlag},
    utilities::point_to_angle_2,
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
        if
        // TODO: gameversion > exe_doom_1_2 &&
        actor.target.is_none() {
            actor.threshold -= 1;
        } else {
            // unsafe { actor.target.as_ref().unwrap().health <= 0 }
            if let Some(target) = actor.target {
                unsafe {
                    if (*target).health <= 0 {
                        actor.threshold = 0;
                    }
                }
            }
        }
    }

    if let Some(target) = actor.target {
        unsafe {
            // Inanimate object, try to find new target
            if (*target).flags & MapObjectFlag::Shootable as u32 == 0 {
                // TODO: if (P_LookForPlayers(actor, true))
                // return;
                actor.set_state(actor.info.spawnstate);
                return;
            }
        }
    } else {
        // No target, let's look
        // TODO: if (P_LookForPlayers(actor, true))
        // return;
        actor.set_state(actor.info.spawnstate);
        return;
    }

    if actor.flags & MapObjectFlag::JustAttacked as u32 != 0 {
        actor.flags ^= MapObjectFlag::JustAttacked as u32;
        // if (gameskill != sk_nightmare && !fastparm)
        //     P_NewChaseDir(actor);

        // TODO: TEMPORARY TESTING LINE
        actor.set_state(actor.info.spawnstate);
        return;
    }

    // Melee attack?
    if actor.info.meleestate != StateNum::S_NULL {
        // TODO: && P_CheckMeleeRange(actor)
        if actor.info.attacksound != SfxEnum::None {
            actor.start_sound(actor.info.attacksound);
        }
        actor.set_state(actor.info.meleestate);
    }

    // Missile attack?
    if actor.info.missilestate != StateNum::S_NULL {
        // if (gameskill < sk_nightmare && !fastparm && actor->movecount)
        // {
        // goto nomissile;
        // }
        // if (!P_CheckMissileRange(actor))
        // goto nomissile;
        //
        actor.flags |= MapObjectFlag::JustAttacked as u32;
        actor.set_state(actor.info.missilestate);
        return;
    }

    // // ?
    // nomissile:
    // // possibly choose another target
    // if (netgame && !actor->threshold && !P_CheckSight(actor, actor->target))
    // {
    // if (P_LookForPlayers(actor, true))
    // return; // got a new target
    // }
    //
    // // chase towards player
    // if (--actor->movecount < 0 || !P_Move(actor))
    // {
    // P_NewChaseDir(actor);
    // }
    //
    // make active sound
    if actor.info.activesound != SfxEnum::None && p_random() < 3 {
        actor.start_sound(actor.info.activesound);
    }
}

/// Stay in state until a player is sighted.
pub fn a_look(actor: &mut MapObject) {
    //error!("a_look not implemented");
    // mobj_t *targ;
    //
    actor.threshold = 0; // any shot will wake up
                         // targ = actor->subsector->sector->soundtarget;
                         //
                         // if (targ && (targ->flags & SHOOTABLE))
                         // {
                         // actor->target = targ;
                         //
                         // if (actor->flags & AMBUSH)
                         // {
                         // if (P_CheckSight(actor, actor->target))
                         // goto seeyou;
                         // }
                         // else
                         // goto seeyou;
                         // }
                         //
                         // if (!P_LookForPlayers(actor, false))
                         // return;
                         //
                         // // go into chase state
                         // seeyou:
                         // if (actor->info->seesound)
                         // {
                         // int sound;
                         //
                         // switch (actor->info->seesound)
                         // {
                         // case sfx_posit1:
                         // case sfx_posit2:
                         // case sfx_posit3:
                         // sound = sfx_posit1 + P_Random() % 3;
                         // break;
                         //
                         // case sfx_bgsit1:
                         // case sfx_bgsit2:
                         // sound = sfx_bgsit1 + P_Random() % 2;
                         // break;
                         //
                         // default:
                         // sound = actor->info->seesound;
                         // break;
                         // }
                         //
                         // if (actor->type == MT_SPIDER || actor->type == MT_CYBORG)
                         // {
                         // // full volume
                         // S_StartSound(NULL, sound);
                         // }
                         // else
                         // S_StartSound(actor, sound);
                         // }
                         //
                         // P_SetMobjState(actor, actor->info->seestate);
                         // actor.set_state(actor.info.seestate);
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
    error!("a_skullattack not implemented");
}

pub fn a_headattack(actor: &mut MapObject) {
    error!("a_headattack not implemented");
}

pub fn a_sargattack(actor: &mut MapObject) {
    error!("a_sargattack not implemented");
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
    if actor.target.is_none() {
        return;
    }

    a_facetarget(actor);
    error!("a_troopattack not implemented");
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
