//! Doom source name `p_pspr`

use log::error;

use super::{
    mobj::MapObject,
    player::{Player, PsprNum},
};

use crate::{
    doom_def::{MISSILERANGE, WEAPON_INFO},
    info::{State, StateNum, STATES},
    tic_cmd::TIC_CMD_BUTTONS,
    PlayerState, WeaponType,
};

const LOWERSPEED: f32 = 6.0;
const RAISESPEED: f32 = 6.0;
pub(crate) const WEAPONBOTTOM: f32 = 128.0;
const WEAPONTOP: f32 = 32.0;

/// From P_PSPR
#[derive(Debug)]
pub struct PspDef {
    /// a NULL state means not active
    pub state: Option<&'static State>,
    pub tics: i32,
    pub sx: f32,
    pub sy: f32,
}

/// The player can re-fire the weapon
/// without lowering it entirely.
pub fn a_refire(actor: &mut Player, _pspr: &mut PspDef) {
    if actor.cmd.buttons & TIC_CMD_BUTTONS.bt_attack != 0
        && actor.pendingweapon == WeaponType::NoChange
        && actor.health != 0
    {
        actor.refire += 1;
        actor.fire_weapon();
    } else {
        actor.refire = 0;
        actor.check_ammo();
    }

    error!("a_refire not completed");
}

pub fn a_weaponready(actor: &mut Player, pspr: &mut PspDef) {
    let mut level_time = 0;
    if let Some(mobj) = actor.mobj {
        let mobj = unsafe { &mut *mobj };

        if std::ptr::eq(mobj.state, &STATES[StateNum::S_PLAY_ATK1 as usize])
            || std::ptr::eq(mobj.state, &STATES[StateNum::S_PLAY_ATK2 as usize])
        {
            mobj.set_state(StateNum::S_PLAY);
        }

        level_time = unsafe { (*mobj.level).level_time };
    }

    // TODO: if (player->readyweapon == wp_chainsaw && psp->state == &states[S_SAW]) {
    //     S_StartSound(player->mo, sfx_sawidl);
    // }

    // check for change
    //  if player is dead, put the weapon away
    if actor.pendingweapon != WeaponType::NoChange || actor.health <= 0 {
        // change weapon
        //  (pending weapon should allready be validated)
        if actor.readyweapon != WeaponType::NoChange {
            let new_state = WEAPON_INFO[actor.readyweapon as usize].downstate;
            actor.set_psprite(PsprNum::Weapon as usize, new_state);
        }
        return;
    }

    // TODO: TEMPORARY
    if actor.cmd.buttons & TIC_CMD_BUTTONS.bt_attack != 0 {
        if !actor.attackdown {
            actor.attackdown = true;
            actor.fire_weapon();
            return;
        }
    } else {
        actor.attackdown = false;
    }

    // Removed the shifts and division from `angle = (FINEANGLES / 20 * leveltime) & FINEMASK;`
    // finemask = 67100672
    let angle = (level_time as f32) * 0.14;
    pspr.sx = 1.0 + actor.bob * (angle as f32).cos();
    let angle = (level_time as f32) * 0.18;
    pspr.sy = WEAPONTOP + 5.0 + actor.bob * (angle as f32).sin() * 0.1;
}

pub fn a_lower(actor: &mut Player, pspr: &mut PspDef) {
    pspr.sy += LOWERSPEED;
    if pspr.sy < WEAPONBOTTOM {
        return;
    }

    if actor.player_state == PlayerState::Dead {
        // Keep weapon down if dead
        pspr.sy = WEAPONBOTTOM;
        return;
    }

    if actor.health <= 0 {
        // Player died so take weapon off screen
        actor.set_psprite(PsprNum::Weapon as usize, StateNum::S_NULL);
        return;
    }

    actor.readyweapon = actor.pendingweapon;
    actor.bring_up_weapon();
}

pub fn a_raise(actor: &mut Player, pspr: &mut PspDef) {
    pspr.sy -= RAISESPEED;
    if pspr.sy > WEAPONTOP {
        return;
    }
    pspr.sy = WEAPONTOP;

    let new_state = WEAPON_INFO[actor.readyweapon as usize].readystate;
    actor.set_psprite(PsprNum::Weapon as usize, new_state);
}

pub fn a_firepistol(actor: &mut Player, _pspr: &mut PspDef) {
    let distance = MISSILERANGE;
    // TODO: S_StartSound(player->mo, sfx_pistol);

    if let Some(mobj) = actor.mobj {
        let mobj = unsafe { &mut *mobj };

        mobj.set_state(StateNum::S_PLAY_ATK2);
        actor.ammo[WEAPON_INFO[actor.readyweapon as usize].ammo as usize] -= 1;
        actor.set_psprite(
            PsprNum::Flash as usize,
            WEAPON_INFO[actor.readyweapon as usize].flashstate,
        );

        let mut bsp_trace = mobj.get_shoot_bsp_trace(distance);
        let bullet_slope = mobj.bullet_slope(distance, &mut bsp_trace);
        // TODO: !player->refire
        mobj.gun_shot(true, distance, bullet_slope, &mut bsp_trace);
    }
}

pub fn a_fireshotgun(actor: &mut Player, _pspr: &mut PspDef) {
    let distance = MISSILERANGE;

    if let Some(mobj) = actor.mobj {
        let mobj = unsafe { &mut *mobj };

        mobj.set_state(StateNum::S_PLAY_ATK2);
        actor.ammo[WEAPON_INFO[actor.readyweapon as usize].ammo as usize] -= 1;
        actor.set_psprite(
            PsprNum::Flash as usize,
            WEAPON_INFO[actor.readyweapon as usize].flashstate,
        );

        let mut bsp_trace = mobj.get_shoot_bsp_trace(distance);
        let bullet_slope = mobj.bullet_slope(distance, &mut bsp_trace);

        for _ in 0..7 {
            mobj.gun_shot(false, distance, bullet_slope.clone(), &mut bsp_trace);
        }
    }
}

pub fn a_fireshotgun2(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub fn a_firecgun(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub fn a_fireplasma(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub fn a_firemissile(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub fn a_firebfg(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub fn a_bfgsound(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub fn a_gunflash(actor: &mut Player, _pspr: &mut PspDef) {
    actor.set_mobj_state(StateNum::S_PLAY_ATK2);
    actor.set_psprite(
        PsprNum::Flash as usize,
        WEAPON_INFO[actor.readyweapon as usize].flashstate,
    );
}

pub fn a_punch(actor: &mut Player, _pspr: &mut PspDef) {
    error!("a_punch not implemented");
}

pub fn a_checkreload(actor: &mut Player, _pspr: &mut PspDef) {
    actor.check_ammo();
}

pub fn a_openshotgun2(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub fn a_loadshotgun2(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub fn a_closeshotgun2(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub fn a_saw(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub fn a_light0(actor: &mut Player, _pspr: &mut PspDef) {
    actor.extralight = 0;
}

pub fn a_light1(actor: &mut Player, _pspr: &mut PspDef) {
    actor.extralight = 1;
}

pub fn a_light2(actor: &mut Player, _pspr: &mut PspDef) {
    actor.extralight = 2;
}

pub fn a_bfgspray(actor: &mut MapObject) {
    unimplemented!()
}
