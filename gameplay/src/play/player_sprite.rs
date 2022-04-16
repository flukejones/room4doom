//! Doom source name `p_pspr`

use std::f32::consts::FRAC_PI_2;

use log::error;
use sound_traits::SfxEnum;

use super::{
    mobj::MapObject,
    player::{Player, PsprNum},
};

use crate::{
    doom_def::{PowerType, MELEERANGE, MISSILERANGE, WEAPON_INFO},
    info::{State, StateNum, STATES},
    play::utilities::{p_random, point_to_angle_2},
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
pub fn a_refire(player: &mut Player, _pspr: &mut PspDef) {
    if player.cmd.buttons & TIC_CMD_BUTTONS.bt_attack != 0
        && player.pendingweapon == WeaponType::NoChange
        && player.health != 0
    {
        player.refire += 1;
        player.fire_weapon();
    } else {
        player.refire = 0;
        player.check_ammo();
    }
}

pub fn a_weaponready(player: &mut Player, pspr: &mut PspDef) {
    let mut level_time = 0;
    if let Some(mobj) = player.mobj {
        let mobj = unsafe { &mut *mobj };

        if std::ptr::eq(mobj.state, &STATES[StateNum::S_PLAY_ATK1 as usize])
            || std::ptr::eq(mobj.state, &STATES[StateNum::S_PLAY_ATK2 as usize])
        {
            mobj.set_state(StateNum::S_PLAY);
        }

        level_time = unsafe { (*mobj.level).level_time };

        if let Some(state) = pspr.state {
            let check = &STATES[StateNum::S_SAW as usize];
            if player.readyweapon == WeaponType::Chainsaw
                && state.sprite == check.sprite
                && state.frame == check.frame
                && state.next_state == check.next_state
            {
                mobj.start_sound(SfxEnum::sawidl);
            }
        }
    }

    // check for change
    //  if player is dead, put the weapon away
    if player.pendingweapon != WeaponType::NoChange || player.health <= 0 {
        // change weapon
        //  (pending weapon should allready be validated)
        if player.readyweapon != WeaponType::NoChange {
            let new_state = WEAPON_INFO[player.readyweapon as usize].downstate;
            player.set_psprite(PsprNum::Weapon as usize, new_state);
        }
        return;
    }

    // TODO: TEMPORARY
    if player.cmd.buttons & TIC_CMD_BUTTONS.bt_attack != 0 {
        if !player.attackdown
            || (player.readyweapon != WeaponType::Missile && player.readyweapon != WeaponType::BFG)
        {
            player.attackdown = true;
            player.fire_weapon();
            return;
        }
    } else {
        player.attackdown = false;
    }

    let angle = (level_time as f32) * 0.1;
    pspr.sx = 1.0 + player.bob * (angle as f32).cos();
    let angle = (level_time as f32) * 0.2;
    pspr.sy = WEAPONTOP + 5.0 + player.bob * (angle as f32).sin() * 0.1;
}

pub fn a_lower(player: &mut Player, pspr: &mut PspDef) {
    pspr.sy += LOWERSPEED;
    if pspr.sy < WEAPONBOTTOM {
        return;
    }

    if player.player_state == PlayerState::Dead {
        // Keep weapon down if dead
        pspr.sy = WEAPONBOTTOM;
        return;
    }

    if player.health <= 0 {
        // Player died so take weapon off screen
        player.set_psprite(PsprNum::Weapon as usize, StateNum::S_NULL);
        return;
    }

    player.readyweapon = player.pendingweapon;
    player.bring_up_weapon();
}

pub fn a_raise(player: &mut Player, pspr: &mut PspDef) {
    pspr.sy -= RAISESPEED;
    if pspr.sy > WEAPONTOP {
        return;
    }
    pspr.sy = WEAPONTOP;

    let new_state = WEAPON_INFO[player.readyweapon as usize].readystate;
    player.set_psprite(PsprNum::Weapon as usize, new_state);
}

pub fn a_firepistol(player: &mut Player, _pspr: &mut PspDef) {
    let distance = MISSILERANGE;

    if let Some(mobj) = player.mobj {
        let mobj = unsafe { &mut *mobj };

        mobj.start_sound(SfxEnum::pistol);
        mobj.set_state(StateNum::S_PLAY_ATK2);
        player.ammo[WEAPON_INFO[player.readyweapon as usize].ammo as usize] -= 1;
        player.set_psprite(
            PsprNum::Flash as usize,
            WEAPON_INFO[player.readyweapon as usize].flashstate,
        );

        let mut bsp_trace = mobj.get_shoot_bsp_trace(distance);
        let bullet_slope = mobj.bullet_slope(distance, &mut bsp_trace);
        mobj.gun_shot(player.refire == 0, distance, bullet_slope, &mut bsp_trace);
    }
}

pub fn a_fireshotgun(player: &mut Player, _pspr: &mut PspDef) {
    let distance = MISSILERANGE;

    if let Some(mobj) = player.mobj {
        let mobj = unsafe { &mut *mobj };

        mobj.start_sound(SfxEnum::shotgn);
        mobj.set_state(StateNum::S_PLAY_ATK2);
        player.subtract_readyweapon_ammo(1);
        player.set_psprite(
            PsprNum::Flash as usize,
            WEAPON_INFO[player.readyweapon as usize].flashstate,
        );

        let mut bsp_trace = mobj.get_shoot_bsp_trace(distance);
        let bullet_slope = mobj.bullet_slope(distance, &mut bsp_trace);

        for _ in 0..7 {
            mobj.gun_shot(false, distance, bullet_slope.clone(), &mut bsp_trace);
        }
    }
}

pub fn a_fireshotgun2(player: &mut Player, _pspr: &mut PspDef) {
    let distance = MISSILERANGE;

    if let Some(mobj) = player.mobj {
        let mobj = unsafe { &mut *mobj };

        mobj.start_sound(SfxEnum::dshtgn);
        mobj.set_state(StateNum::S_PLAY_ATK2);
        player.subtract_readyweapon_ammo(2);
        player.set_psprite(
            PsprNum::Flash as usize,
            WEAPON_INFO[player.readyweapon as usize].flashstate,
        );

        let mut bsp_trace = mobj.get_shoot_bsp_trace(distance);
        let bullet_slope = mobj.bullet_slope(distance, &mut bsp_trace);

        for _ in 0..20 {
            let damage = 5.0 * (p_random() % 3 + 1) as f32;
            let mut angle = mobj.angle;
            angle += (((p_random() - p_random()) >> 5) as f32).to_radians();
            mobj.line_attack(
                damage,
                MISSILERANGE,
                angle,
                bullet_slope.clone(),
                &mut bsp_trace,
            );
        }
    }
}

pub fn a_firecgun(player: &mut Player, pspr: &mut PspDef) {
    if !player.check_ammo() {
        return;
    }

    if let Some(mobj) = player.mobj {
        let mobj = unsafe { &mut *mobj };

        mobj.start_sound(SfxEnum::pistol);
        mobj.set_state(StateNum::S_PLAY_ATK2);
        player.subtract_readyweapon_ammo(1);

        let state = StateNum::from(
            WEAPON_INFO[player.readyweapon as usize].flashstate as u16
                + pspr.state.unwrap().next_state as u16
                - StateNum::S_CHAIN1 as u16
                - 1,
        );
        player.set_psprite(PsprNum::Flash as usize, state);

        let mut bsp_trace = mobj.get_shoot_bsp_trace(MISSILERANGE);
        let bullet_slope = mobj.bullet_slope(MISSILERANGE, &mut bsp_trace);
        mobj.gun_shot(
            player.refire == 0,
            MISSILERANGE,
            bullet_slope,
            &mut bsp_trace,
        );
    }
}

pub fn a_fireplasma(player: &mut Player, _pspr: &mut PspDef) {
    player.subtract_readyweapon_ammo(1);
    let state = StateNum::from(
        (WEAPON_INFO[player.readyweapon as usize].flashstate as u16 + p_random() as u16) & 1,
    );
    player.set_psprite(PsprNum::Flash as usize, state);
    if let Some(mobj) = player.mobj {
        unsafe {
            (*mobj).start_sound(SfxEnum::plasma);
            MapObject::spawn_player_missile(
                &mut *mobj,
                crate::MapObjectType::MT_PLASMA,
                &mut (*(*mobj).level),
            );
        }
    }
}

pub fn a_firemissile(player: &mut Player, _pspr: &mut PspDef) {
    player.subtract_readyweapon_ammo(1);
    // player.set_psprite(
    //     PsprNum::Flash as usize,
    //     WEAPON_INFO[player.readyweapon as usize].flashstate,
    // );
    if let Some(mobj) = player.mobj {
        unsafe {
            (*mobj).start_sound(SfxEnum::rlaunc);
            MapObject::spawn_player_missile(
                &mut *mobj,
                crate::MapObjectType::MT_ROCKET,
                &mut (*(*mobj).level),
            );
        }
    }
}

pub fn a_firebfg(player: &mut Player, _pspr: &mut PspDef) {
    player.subtract_readyweapon_ammo(1);
    // player.set_psprite(
    //     PsprNum::Flash as usize,
    //     WEAPON_INFO[player.readyweapon as usize].flashstate,
    // );
    if let Some(mobj) = player.mobj {
        unsafe {
            MapObject::spawn_player_missile(
                &mut *mobj,
                crate::MapObjectType::MT_BFG,
                &mut (*(*mobj).level),
            );
        }
    }
}

pub fn a_bfgsound(player: &mut Player, _pspr: &mut PspDef) {
    if let Some(mobj) = player.mobj {
        unsafe {
            (*mobj).start_sound(SfxEnum::bfg);
        }
    }
}

pub fn a_bfgspray(player: &mut MapObject) {
    error!("TODO: a_bfgspray not implemented");
}

pub fn a_gunflash(player: &mut Player, _pspr: &mut PspDef) {
    player.set_mobj_state(StateNum::S_PLAY_ATK2);
    player.set_psprite(
        PsprNum::Flash as usize,
        WEAPON_INFO[player.readyweapon as usize].flashstate,
    );
}

pub fn a_punch(player: &mut Player, _pspr: &mut PspDef) {
    let mut damage = (p_random() % 10 + 1) as f32;
    if player.powers[PowerType::Strength as usize] != 0 {
        damage *= 10.0;
    }

    if let Some(mobj) = player.mobj {
        let mobj = unsafe { &mut *mobj };

        let mut angle = mobj.angle;
        angle += (((p_random() - p_random()) >> 5) as f32).to_radians();

        let mut bsp_trace = mobj.get_shoot_bsp_trace(MELEERANGE);
        let slope = mobj.aim_line_attack(MELEERANGE, &mut bsp_trace);
        mobj.line_attack(damage, MELEERANGE, angle, slope.clone(), &mut bsp_trace);

        if let Some(res) = slope {
            let target = res.line_target;
            mobj.start_sound(SfxEnum::punch);
            mobj.angle = point_to_angle_2(target.xy, mobj.xy);
        }
    }
}

pub fn a_checkreload(player: &mut Player, _pspr: &mut PspDef) {
    player.check_ammo();
}

pub fn a_openshotgun2(player: &mut Player, _pspr: &mut PspDef) {
    if let Some(mobj) = player.mobj {
        unsafe {
            (*mobj).start_sound(SfxEnum::dbopn);
        }
    }
}

pub fn a_loadshotgun2(player: &mut Player, _pspr: &mut PspDef) {
    if let Some(mobj) = player.mobj {
        unsafe {
            (*mobj).start_sound(SfxEnum::dbload);
        }
    }
}

pub fn a_closeshotgun2(player: &mut Player, pspr: &mut PspDef) {
    if let Some(mobj) = player.mobj {
        unsafe {
            (*mobj).start_sound(SfxEnum::dbcls);
        }
    }
    a_refire(player, pspr);
}

pub fn a_saw(player: &mut Player, _pspr: &mut PspDef) {
    let damage = 2.0 * (p_random() % 10 + 1) as f32;

    if let Some(mobj) = player.mobj {
        let mobj = unsafe { &mut *mobj };
        let mut angle = mobj.angle;
        angle += (((p_random() - p_random()) >> 5) as f32).to_radians();

        let mut bsp_trace = mobj.get_shoot_bsp_trace(MELEERANGE + 1.0);
        let slope = mobj.aim_line_attack(MELEERANGE + 1.0, &mut bsp_trace);
        mobj.line_attack(
            damage,
            MELEERANGE + 1.0,
            angle,
            slope.clone(),
            &mut bsp_trace,
        );

        if slope.is_none() {
            mobj.start_sound(SfxEnum::sawful);
            return;
        }

        // Have a target
        mobj.start_sound(SfxEnum::sawhit);
        if let Some(res) = slope {
            let target = res.line_target;
            mobj.start_sound(SfxEnum::punch);
            let angle = point_to_angle_2(target.xy, mobj.xy);

            let delta = angle.rad() - mobj.angle.rad();
            if delta > FRAC_PI_2 / 20.0 {
                mobj.angle += FRAC_PI_2 / 21.0;
            } else {
                mobj.angle -= FRAC_PI_2 / 20.0;
            }
        }
    }
}

pub fn a_light0(player: &mut Player, _pspr: &mut PspDef) {
    player.extralight = 0;
}

pub fn a_light1(player: &mut Player, _pspr: &mut PspDef) {
    player.extralight = 1;
}

pub fn a_light2(player: &mut Player, _pspr: &mut PspDef) {
    player.extralight = 2;
}
