use crate::d_thinker::ThinkerType;
use crate::level_data::level::Level;
use crate::p_map_object::MobjFlag;
use crate::renderer::bsp::point_to_angle_2;
use crate::{p_map::p_radius_attack, p_map_object::MapObject};

/// A_FaceTarget
pub fn a_facetarget(actor: &mut MapObject) {
    actor.flags &= !(MobjFlag::AMBUSH as u32);

    unsafe {
        let angle = point_to_angle_2(&actor.xy, &actor.target.unwrap().as_mut().xy);
        actor.angle = angle;

        if actor.target.unwrap().as_mut().flags & MobjFlag::SHADOW as u32 == MobjFlag::SHADOW as u32
        {
            // TODO: actor.angle += P_SubRandom() << 21;
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
        actor.target.is_none()
            || (actor.target.is_some() && unsafe { actor.target.unwrap().as_ref().health <= 0 })
        {
            actor.threshold = 0;
        } else {
            actor.threshold -= 1;
        }
    }

    unimplemented!()
    //

    //
    // // turn towards movement direction if not there yet
    // if (actor->movedir < 8)
    // {
    // actor->angle &= (7 << 29);
    // delta = actor->angle - (actor->movedir << 29);
    //
    // if (delta > 0)
    // actor->angle -= ANG90 / 2;
    // else if (delta < 0)
    // actor->angle += ANG90 / 2;
    // }
    //
    // if (!actor->target || !(actor->target->flags & SHOOTABLE))
    // {
    // // look for a new target
    // if (P_LookForPlayers(actor, true))
    // return; // got a new target
    //
    // P_SetMobjState(actor, actor->info->spawnstate);
    // return;
    // }
    //
    // // do not attack twice in a row
    // if (actor->flags & JUSTATTACKED)
    // {
    // actor->flags &= ~JUSTATTACKED;
    // if (gameskill != sk_nightmare && !fastparm)
    // P_NewChaseDir(actor);
    // return;
    // }
    //
    // // check for melee attack
    // if (actor->info->meleestate && P_CheckMeleeRange(actor))
    // {
    // if (actor->info->attacksound)
    // S_StartSound(actor, actor->info->attacksound);
    //
    // P_SetMobjState(actor, actor->info->meleestate);
    // return;
    // }
    //
    // // check for missile attack
    // if (actor->info->missilestate)
    // {
    // if (gameskill < sk_nightmare && !fastparm && actor->movecount)
    // {
    // goto nomissile;
    // }
    //
    // if (!P_CheckMissileRange(actor))
    // goto nomissile;
    //
    // P_SetMobjState(actor, actor->info->missilestate);
    // actor->flags |= JUSTATTACKED;
    // return;
    // }
    //
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
    // // make active sound
    // if (actor->info->activesound && P_Random() < 3)
    // {
    // S_StartSound(actor, actor->info->activesound);
    // }
}

/// Stay in state until a player is sighted.
pub fn a_look(_actor: &mut MapObject) {
    unimplemented!()
    // mobj_t *targ;
    //
    // actor->threshold = 0; // any shot will wake up
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
}

pub fn a_fire(_actor: &mut MapObject) {
    unimplemented!()
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

pub fn a_scream(_actor: &mut MapObject) {
    unimplemented!()
    // int sound;
    //
    // switch (actor->info->deathsound)
    // {
    // case 0:
    // return;
    //
    // case sfx_podth1:
    // case sfx_podth2:
    // case sfx_podth3:
    // sound = sfx_podth1 + P_Random() % 3;
    // break;
    //
    // case sfx_bgdth1:
    // case sfx_bgdth2:
    // sound = sfx_bgdth1 + P_Random() % 2;
    // break;
    //
    // default:
    // sound = actor->info->deathsound;
    // break;
    // }
    //
    // // Check for bosses.
    // if (actor->type == MT_SPIDER || actor->type == MT_CYBORG)
    // {
    // // full volume
    // S_StartSound(NULL, sound);
    // }
    // else
    // S_StartSound(actor, sound);
}

pub fn a_pain(actor: &mut ThinkerType, level: &mut Level) -> bool {
    let actor = actor.bad_mut::<MapObject>();
    unimplemented!()
    // if (actor->info->painsound)
    // S_StartSound(actor, actor->info->painsound);
}

pub fn a_fall(actor: &mut MapObject) {
    // actor is on ground, it can be walked over
    actor.flags &= !(MobjFlag::SOLID as u32);

    // So change this if corpse objects
    // are meant to be obstacles.
}

pub fn a_explode(actor: &mut MapObject) {
    if let Some(mut target) = actor.target {
        // just casually breaking lifetimes
        let target = unsafe { target.as_mut() };
        p_radius_attack(actor, target, 128.0);
    }
}

pub fn a_xscream(_actor: &mut MapObject) {
    unimplemented!()
    // if (actor->info->painsound)
    // S_StartSound(actor, actor->info->painsound);
}
