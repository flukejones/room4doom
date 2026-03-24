//! Doom source name `p_pspr`

use sound_common::SfxName;

use crate::doom_def::{MELEERANGE, MISSILERANGE, PowerType, WEAPON_INFO};
use crate::info::{STATES, StateData, StateNum};
use crate::player::{Player, PsprNum};
use crate::thing::MapObject;
use crate::{MapObjKind, PlayerState};
use game_config::WeaponType;
use game_config::tic_cmd::TIC_CMD_BUTTONS;
use math::{ANG90, ANG180, Angle, FixedT, p_random, r_point_to_angle};

const ANG90_I32: i32 = ANG90 as i32;
const LOWERSPEED: f32 = 6.0;
const RAISESPEED: f32 = 6.0;
pub(crate) const WEAPONBOTTOM: f32 = 128.0;
const WEAPONTOP: f32 = 32.0;

/// From P_PSPR
#[derive(Debug, Clone, Copy)]
pub struct PspDef {
    /// a NULL state means not active
    pub state: Option<&'static StateData>,
    pub(crate) tics: i32,
    pub sx: f32,
    pub sy: f32,
}

/// The player can re-fire the weapon
/// without lowering it entirely.
pub(crate) fn a_refire(player: &mut Player, _pspr: &mut PspDef) {
    if player.cmd.buttons & TIC_CMD_BUTTONS.bt_attack != 0
        && player.pendingweapon == WeaponType::NoChange
        && player.status.health != 0
    {
        player.refire += 1;
        player.fire_weapon();
    } else {
        player.refire = 0;
        player.check_ammo();
    }
}

pub(crate) fn a_weaponready(player: &mut Player, pspr: &mut PspDef) {
    let mut level_time = 0;
    let readyweapon = player.status.readyweapon;
    if let Some(mobj) = player.mobj_mut() {
        if std::ptr::eq(mobj.state, &STATES[StateNum::PLAY_ATK1 as usize])
            || std::ptr::eq(mobj.state, &STATES[StateNum::PLAY_ATK2 as usize])
        {
            mobj.set_state(StateNum::PLAY);
        }

        level_time = unsafe { (*mobj.level).level_time };

        if let Some(state) = pspr.state {
            let check = &STATES[StateNum::SAW as usize];
            if readyweapon == WeaponType::Chainsaw
                && state.sprite == check.sprite
                && state.frame == check.frame
                && state.next_state == check.next_state
            {
                mobj.start_sound(SfxName::Sawidl);
            }
        }
    }

    // check for change
    //  if player is dead, put the weapon away
    if player.pendingweapon != WeaponType::NoChange || player.status.health <= 0 {
        // change weapon
        //  (pending weapon should allready be validated)
        if player.status.readyweapon != WeaponType::NoChange {
            let new_state = WEAPON_INFO[player.status.readyweapon as usize].downstate;
            player.set_psprite(PsprNum::Weapon as usize, new_state);
        }
        return;
    }

    //  the missile launcher and bfg do not auto fire
    if player.cmd.buttons & TIC_CMD_BUTTONS.bt_attack != 0 {
        if !player.status.attackdown
            || (player.status.readyweapon != WeaponType::Missile
                && player.status.readyweapon != WeaponType::BFG)
        {
            player.status.attackdown = true;
            player.fire_weapon();
            return;
        }
    } else {
        player.status.attackdown = false;
    }

    // OG: angle = (128 * leveltime) & FINEMASK
    let fine_angle = ((128 * level_time) & 8191) as usize;
    let bob = player.bob;
    // OG: sx = FRACUNIT + FixedMul(bob, finecosine[angle])
    let sx = FixedT::ONE + bob.fixed_mul(math::finecosine(fine_angle));
    pspr.sx = math::fixed_to_float(sx.to_fixed_raw());
    // OG: angle &= (FINEANGLES/2 - 1)
    let fine_angle = fine_angle & 4095;
    // OG: sy = WEAPONTOP + FixedMul(bob, finesine[angle])
    let sy = FixedT::from_fixed(math::float_to_fixed(WEAPONTOP))
        + bob.fixed_mul(math::finesine(fine_angle));
    pspr.sy = math::fixed_to_float(sy.to_fixed_raw());
}

pub(crate) fn a_lower(player: &mut Player, pspr: &mut PspDef) {
    pspr.sy += LOWERSPEED;
    if pspr.sy < WEAPONBOTTOM {
        return;
    }

    if player.player_state == PlayerState::Dead {
        // Keep weapon down if dead
        pspr.sy = WEAPONBOTTOM;
        return;
    }

    if player.status.health <= 0 {
        // Player died so take weapon off screen
        player.set_psprite(PsprNum::Weapon as usize, StateNum::None);
        return;
    }

    player.status.readyweapon = player.pendingweapon;
    player.bring_up_weapon();
}

pub(crate) fn a_raise(player: &mut Player, pspr: &mut PspDef) {
    pspr.sy -= RAISESPEED;
    if pspr.sy > WEAPONTOP {
        return;
    }
    pspr.sy = WEAPONTOP;

    let new_state = WEAPON_INFO[player.status.readyweapon as usize].readystate;
    player.set_psprite(PsprNum::Weapon as usize, new_state);
}

fn shoot_bullet(player: &mut Player) {
    let distance: FixedT = MISSILERANGE.into();
    let refire = player.refire;
    if let Some(mobj) = player.mobj_mut() {
        mobj.start_sound(SfxName::Pistol);
        mobj.set_state(StateNum::PLAY_ATK2);

        let mut bsp_trace = mobj.get_shoot_bsp_trace(distance);
        let bullet_slope = mobj.bullet_slope(distance, &mut bsp_trace);
        mobj.gun_shot(refire == 0, distance, bullet_slope, &mut bsp_trace);
    }
}

pub(crate) fn a_firepistol(player: &mut Player, _pspr: &mut PspDef) {
    shoot_bullet(player);
    player.status.ammo[WEAPON_INFO[player.status.readyweapon as usize].ammo as usize] -= 1;
    player.set_psprite(
        PsprNum::Flash as usize,
        WEAPON_INFO[player.status.readyweapon as usize].flashstate,
    );
}

pub(crate) fn a_fireshotgun(player: &mut Player, _pspr: &mut PspDef) {
    let distance: FixedT = MISSILERANGE.into();

    if let Some(mobj) = player.mobj_mut() {
        mobj.start_sound(SfxName::Shotgn);
        mobj.set_state(StateNum::PLAY_ATK2);

        let mut bsp_trace = mobj.get_shoot_bsp_trace(distance);
        let bullet_slope = mobj.bullet_slope(distance, &mut bsp_trace);

        for _ in 0..7 {
            mobj.gun_shot(false, distance, bullet_slope.clone(), &mut bsp_trace);
        }
    }

    player.subtract_readyweapon_ammo(1);
    player.set_psprite(
        PsprNum::Flash as usize,
        WEAPON_INFO[player.status.readyweapon as usize].flashstate,
    );
}

pub(crate) fn a_fireshotgun2(player: &mut Player, _pspr: &mut PspDef) {
    let distance: FixedT = MISSILERANGE.into();

    if let Some(mobj) = player.mobj_mut() {
        mobj.start_sound(SfxName::Dshtgn);
        mobj.set_state(StateNum::PLAY_ATK2);

        let mut bsp_trace = mobj.get_shoot_bsp_trace(distance);
        let bullet_slope = mobj.bullet_slope(distance, &mut bsp_trace);

        for _ in 0..20 {
            let damage = 5 * (p_random() % 3 + 1);
            // OG: angle += (P_Random()-P_Random())<<19
            let spread = ((p_random() - p_random()) << 19) as u32;
            let angle = Angle::from_bam(mobj.angle.to_bam().wrapping_add(spread));
            // OG: bulletslope + ((P_Random()-P_Random())<<5) — slope adjusted per pellet
            let slope_adj = bullet_slope.clone().map(|mut res| {
                let adj = FixedT::from_fixed((p_random() - p_random()) << 5);
                res.aimslope = res.aimslope + adj;
                res
            });
            mobj.line_attack(damage, distance, angle, slope_adj, &mut bsp_trace);
        }
    }

    player.subtract_readyweapon_ammo(2);
    player.set_psprite(
        PsprNum::Flash as usize,
        WEAPON_INFO[player.status.readyweapon as usize].flashstate,
    );
}

pub(crate) fn a_firecgun(player: &mut Player, pspr: &mut PspDef) {
    if !player.check_ammo() {
        return;
    }
    shoot_bullet(player);
    let state = StateNum::from(
        WEAPON_INFO[player.status.readyweapon as usize].flashstate as u16
            + pspr.state.unwrap().next_state as u16
            - StateNum::CHAIN1 as u16
            - 1,
    );

    player.subtract_readyweapon_ammo(1);
    player.set_psprite(PsprNum::Flash as usize, state);
}

pub(crate) fn a_fireplasma(player: &mut Player, _pspr: &mut PspDef) {
    player.subtract_readyweapon_ammo(1);
    let state = StateNum::from(
        (WEAPON_INFO[player.status.readyweapon as usize].flashstate as u16 + p_random() as u16) & 1,
    );
    player.set_psprite(PsprNum::Flash as usize, state);
    if let Some(mobj) = player.mobj_raw() {
        unsafe {
            (*mobj).start_sound(SfxName::Plasma);
            MapObject::spawn_player_missile(
                &mut *mobj,
                crate::MapObjKind::MT_PLASMA,
                &mut (*(*mobj).level),
            );
        }
    }
}

pub(crate) fn a_firemissile(player: &mut Player, _pspr: &mut PspDef) {
    player.subtract_readyweapon_ammo(1);
    // player.set_psprite(
    //     PsprNum::Flash as usize,
    //     WEAPON_INFO[player.status.readyweapon as usize].flashstate,
    // );
    if let Some(mobj) = player.mobj_raw() {
        unsafe {
            (*mobj).start_sound(SfxName::Rlaunc);
            MapObject::spawn_player_missile(
                &mut *mobj,
                crate::MapObjKind::MT_ROCKET,
                &mut (*(*mobj).level),
            );
        }
    }
}

pub(crate) fn a_firebfg(player: &mut Player, _pspr: &mut PspDef) {
    player.subtract_readyweapon_ammo(1);
    // player.set_psprite(
    //     PsprNum::Flash as usize,
    //     WEAPON_INFO[player.status.readyweapon as usize].flashstate,
    // );
    if let Some(mobj) = player.mobj_raw() {
        unsafe {
            MapObject::spawn_player_missile(
                &mut *mobj,
                crate::MapObjKind::MT_BFG,
                &mut (*(*mobj).level),
            );
        }
    }
}

pub(crate) fn a_bfgsound(player: &mut Player, _pspr: &mut PspDef) {
    player.start_sound(SfxName::Bfg);
}

pub(crate) fn a_bfgspray(player: &mut MapObject) {
    let distance: FixedT = MISSILERANGE.into();
    for i in 0..40 {
        // From left to right
        // OG: an = mo->angle - ANG90/2 + ANG90/40*i
        let bam = player
            .angle
            .to_bam()
            .wrapping_sub(ANG90 / 2)
            .wrapping_add((ANG90 / 40) * i);
        let angle = Angle::from_bam(bam);
        let mut bsp_trace = player.get_shoot_bsp_trace(distance);
        let old_angle = player.angle;
        player.angle = angle;
        let aim = player.aim_line_attack(distance, &mut bsp_trace);
        player.angle = old_angle;
        if let Some(aim) = aim {
            let mut lt = aim.line_target;
            let level = unsafe { &mut *player.level };
            let z = lt.z + lt.height.shr(2);
            MapObject::spawn_map_object(lt.x, lt.y, z, MapObjKind::MT_EXTRABFG, level);

            let mut damage = 0;
            for _ in 0..15 {
                damage += (p_random() & 7) + 1;
            }
            // OG: P_DamageMobj(linetarget, mo->target, mo->target, damage)
            let source = player.target.map(|t| unsafe { (*t).mobj_mut() });
            let inflictor = source.as_ref().map(|s| (s.x, s.y, s.z));
            lt.p_take_damage(inflictor, source, damage);
        }
    }
}

pub(crate) fn a_gunflash(player: &mut Player, _pspr: &mut PspDef) {
    player.set_mobj_state(StateNum::PLAY_ATK2);
    player.set_psprite(
        PsprNum::Flash as usize,
        WEAPON_INFO[player.status.readyweapon as usize].flashstate,
    );
}

pub(crate) fn a_punch(player: &mut Player, _pspr: &mut PspDef) {
    // OG: damage = (P_Random()%10+1)<<1
    let mut damage = (p_random() % 10 + 1) << 1;
    if player.status.powers[PowerType::Strength as usize] != 0 {
        damage *= 10;
    }

    if let Some(mobj) = player.mobj_mut() {
        // OG: angle += (P_Random()-P_Random())<<18
        let spread = ((p_random() - p_random()) << 18) as u32;
        let angle = Angle::from_bam(mobj.angle.to_bam().wrapping_add(spread));

        let melee: FixedT = MELEERANGE.into();
        let mut bsp_trace = mobj.get_shoot_bsp_trace(melee);
        // OG: aim uses spread angle
        let old_angle = mobj.angle;
        mobj.angle = angle;
        let slope = mobj.aim_line_attack(melee, &mut bsp_trace);
        mobj.angle = old_angle;
        mobj.line_attack(damage, melee, angle, slope.clone(), &mut bsp_trace);

        if let Some(res) = slope {
            let target = res.line_target;
            mobj.start_sound(SfxName::Punch);
            // OG: R_PointToAngle2
            let dx = target.x - mobj.x;
            let dy = target.y - mobj.y;
            mobj.angle = Angle::from_bam(r_point_to_angle(dx, dy));
        }
    }
}

pub(crate) fn a_checkreload(player: &mut Player, _pspr: &mut PspDef) {
    player.check_ammo();
}

pub(crate) fn a_openshotgun2(player: &mut Player, _pspr: &mut PspDef) {
    player.start_sound(SfxName::Dbopn);
}

pub(crate) fn a_loadshotgun2(player: &mut Player, _pspr: &mut PspDef) {
    player.start_sound(SfxName::Dbload);
}

pub(crate) fn a_closeshotgun2(player: &mut Player, pspr: &mut PspDef) {
    player.start_sound(SfxName::Dbcls);
    a_refire(player, pspr);
}

pub(crate) fn a_saw(player: &mut Player, _pspr: &mut PspDef) {
    let damage = 2 * (p_random() % 10 + 1);

    if let Some(mobj) = player.mobj_mut() {
        // OG: angle += (P_Random()-P_Random())<<18
        let spread = ((p_random() - p_random()) << 18) as u32;
        let angle = Angle::from_bam(mobj.angle.to_bam().wrapping_add(spread));

        let melee: FixedT = (MELEERANGE + 1).into();
        let mut bsp_trace = mobj.get_shoot_bsp_trace(melee);
        // OG: aim uses spread angle
        let old_angle = mobj.angle;
        mobj.angle = angle;
        let slope = mobj.aim_line_attack(melee, &mut bsp_trace);
        mobj.angle = old_angle;
        mobj.line_attack(damage, melee, angle, slope.clone(), &mut bsp_trace);

        if slope.is_none() {
            mobj.start_sound(SfxName::Sawful);
            return;
        }

        mobj.start_sound(SfxName::Sawhit);
        if let Some(res) = slope {
            let target = res.line_target;
            // OG: R_PointToAngle2 + BAM angle adjustment
            let dx = target.x - mobj.x;
            let dy = target.y - mobj.y;
            let targ_angle = r_point_to_angle(dx, dy);
            let delta = targ_angle.wrapping_sub(mobj.angle.to_bam());
            if delta > ANG180 {
                if (delta as i32) < -(ANG90_I32 / 20) {
                    mobj.angle = Angle::from_bam(targ_angle.wrapping_add(ANG90 / 21));
                } else {
                    mobj.angle = Angle::from_bam(mobj.angle.to_bam().wrapping_sub(ANG90 / 20));
                }
            } else if delta > ANG90 / 20 {
                mobj.angle = Angle::from_bam(targ_angle.wrapping_sub(ANG90 / 21));
            } else {
                mobj.angle = Angle::from_bam(mobj.angle.to_bam().wrapping_add(ANG90 / 20));
            }
        }
    }
}

pub(crate) fn a_light0(player: &mut Player, _pspr: &mut PspDef) {
    player.extralight = 0;
}

pub(crate) fn a_light1(player: &mut Player, _pspr: &mut PspDef) {
    player.extralight = 1;
}

pub(crate) fn a_light2(player: &mut Player, _pspr: &mut PspDef) {
    player.extralight = 2;
}
