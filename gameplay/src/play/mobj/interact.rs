//! Environment and object interactions

use std::ptr;

use glam::Vec2;
use log::{debug, error, info};
use sound_traits::SfxEnum;

use super::Skill;
use crate::{
    doom_def::{AmmoType, Card, PowerType, WeaponType},
    info::{MapObjectType, SpriteNum, StateNum, STATES},
    lang::english::*,
    play::{
        mobj::MapObjectFlag,
        player::{PlayerCheat, PlayerState},
        utilities::{p_random, point_to_angle_2},
    },
    MapObject,
};

pub const BONUSADD: i32 = 6;

impl MapObject {
    /// Doom function name `P_DamageMobj`
    ///
    /// - Inflictor is the thing that caused the damage (creature or missle).
    ///   Can be `None` for slime, goo etc.
    /// - Source is the thing to target after taking damage. Should be None for
    ///   things that can't be targetted (environmental).
    /// - Source and inflictor are the same for melee attacks, if this is the case
    ///   then only `source` should be set, and `source_is_inflictor` set to true.
    ///
    /// Relative to the original source, `target` is `self`.
    ///
    /// Self is always the target of the damage to be dealt. So for example if a
    /// Mancubus sends a missile in to a Sargent then:
    ///
    /// - `self` is the Sargent
    /// - `inflictor` is the missile that the Mancubus fired
    /// - `source` is the Mancubus
    /// - The Sargent has it's `target` set to `source`
    ///
    /// If Sargent was a Player then the player has `attacker` set to `source`
    pub(crate) fn p_take_damage(
        &mut self,
        inflictor: Option<&MapObject>,
        mut source: Option<&mut MapObject>,
        source_is_inflictor: bool,
        mut damage: i32,
    ) {
        if self.flags & MapObjectFlag::Shootable as u32 == 0 {
            return;
        }

        if self.health <= 0 {
            return;
        }

        if self.flags & MapObjectFlag::SkullFly as u32 != 0 {
            self.momxy = Vec2::default();
            self.z = 0.0;
        }

        unsafe {
            if self.player.is_some() && (*self.level).game_skill == Skill::Baby {
                damage >>= 1; // take half damage in trainer mode
            }
        }

        // Some close combat weapons should not
        // inflict thrust and push the victim out of reach,
        // thus kick away unless using the chainsaw.
        if inflictor.is_some() || source.is_some() {
            // Source might be inflictor
            let mut do_push = true;
            let inflict = if let Some(inflictor) = inflictor {
                inflictor
            } else {
                // assume source is not None
                // DOn't push away if it's a player with a chainsaw
                do_push = source.as_ref().unwrap().player.is_none()
                    || unsafe {
                        (*source.as_ref().unwrap().player.unwrap()).readyweapon
                            != WeaponType::Chainsaw
                    };
                source.as_mut().unwrap()
            };

            if self.flags & MapObjectFlag::NoClip as u32 == 0 && do_push {
                let mut angle = point_to_angle_2(inflict.xy, self.xy);
                let mut thrust = damage as f32 * 0.001 * 100.0 / self.info.mass as f32;
                // make fall forwards sometimes
                if damage < 40
                    && damage > self.health
                    && self.z - inflict.z > 64.0
                    && p_random() & 1 != 0
                {
                    angle += 180.0; // TODO: verify the results of this (fixed vs float)
                    thrust *= 4.0;
                }

                self.momxy += angle.unit() * thrust;
            }
        }

        if let Some(player) = self.player {
            info!("Ouch!");
            unsafe {
                let mut player = &mut *player;

                // end of game hell hack
                if (*self.subsector).sector.special == 11 && damage >= self.health {
                    damage = self.health - 1;
                }
                // Below certain threshold, ignore damage in GOD mode, or with INVUL power.
                if damage < 1000
                    && (player.cheats & PlayerCheat::Godmode as u32 != 0
                        || player.powers[PowerType::Invulnerability as usize] != 0)
                {
                    return;
                }

                if player.armortype != 0 {
                    let mut saved = if player.armortype == 1 {
                        damage / 3
                    } else {
                        damage / 2
                    };

                    if player.armorpoints <= saved {
                        // armour is used up
                        saved = player.armorpoints;
                        player.armortype = 0;
                    }
                    player.armorpoints -= saved;
                    damage -= saved;
                }

                player.health -= damage;
                if player.health < 0 {
                    player.health = 0;
                }

                if let Some(source) = source.as_mut() {
                    player.attacker = Some(*source);
                }

                player.damagecount += damage;
                if player.damagecount > 100 {
                    player.damagecount = 100; // teleport stomp does 10k points...
                }
                // Tactile feedback thing removed here
            }
        }

        debug!("Applying {damage} damage");
        self.health -= damage;
        if self.health <= 0 {
            self.kill(source);
            return;
        }

        if p_random() < self.info.painchance && self.flags & MapObjectFlag::SkullFly as u32 == 0 {
            self.flags |= MapObjectFlag::JustHit as u32; // FIGHT!!!
            self.set_state(self.info.painstate);
        }

        self.reactiontime = 0; // AWAKE AND READY!

        if self.threshold == 0 || self.kind == MapObjectType::MT_VILE {
            if let Some(source) = source {
                // TODO: gameversion <= exe_doom_1_2
                if !ptr::eq(self, source) && source.kind != MapObjectType::MT_VILE {
                    self.target = Some(source);
                    self.threshold = BASETHRESHOLD;

                    if std::ptr::eq(self.state, &STATES[self.info.spawnstate as usize])
                        && self.info.seestate != StateNum::S_NULL
                    {
                        self.set_state(self.info.seestate);
                    }
                }
            }
        }
    }

    /// Doom function name `P_KillMobj`
    fn kill(&mut self, source: Option<&mut MapObject>) {
        self.flags &= !(MapObjectFlag::Shootable as u32
            | MapObjectFlag::Float as u32
            | MapObjectFlag::SkullFly as u32);

        if self.kind != MapObjectType::MT_SKULL {
            self.flags &= !(MapObjectFlag::NoGravity as u32);
        }

        self.flags |= MapObjectFlag::Corpse as u32 | MapObjectFlag::DropOff as u32;
        self.health >>= 2;

        if let Some(source) = source.as_ref() {
            if let Some(player) = source.player {
                if self.flags & MapObjectFlag::CountKill as u32 != 0 {
                    unsafe {
                        (*player).killcount += 1;
                    }
                }

                if self.player.is_some() {
                    unsafe {
                        // TODO: set correct player for frags
                        (*player).frags[0] += 1;
                    }
                }
            }
        } else {
            // TODO: Need to increment killcount for first player
            //players[0].killcount++;
        }

        if let Some(player) = self.player {
            info!("Killing player");
            unsafe {
                let mut player = &mut *player;
                // Environment kills count against you
                if source.is_none() {
                    // TODO: set correct player for frags
                    (*player).frags[0] += 1;
                }

                self.flags &= !(MapObjectFlag::Solid as u32);
                player.player_state = PlayerState::Dead;
                // TODO: P_DropWeapon(target->player);
                error!("P_DropWeapon not implemented");
                // TODO: stop automap
            }
        }

        if self.health < -self.info.spawnhealth && self.info.xdeathstate != StateNum::S_NULL {
            self.set_state(self.info.xdeathstate);
        } else {
            self.set_state(self.info.deathstate);
        }

        self.tics -= p_random() & 3;
        if self.tics < 1 {
            self.tics = 1;
        }

        let item = match self.kind {
            MapObjectType::MT_WOLFSS | MapObjectType::MT_POSSESSED => MapObjectType::MT_CLIP,
            MapObjectType::MT_SHOTGUY => MapObjectType::MT_SHOTGUN,
            MapObjectType::MT_CHAINGUY => MapObjectType::MT_CHAINGUN,
            _ => return,
        };

        unsafe {
            let mobj = MapObject::spawn_map_object(
                self.xy.x,
                self.xy.y,
                self.floorz as i32,
                item,
                &mut *self.level,
            );
            (*mobj).flags |= MapObjectFlag::Dropped as u32;
        }
    }

    /// Interact with special pickups
    ///
    /// Doom function name `P_TouchSpecialThing`
    pub(super) fn touch_special(&mut self, special: &mut MapObject) {
        let delta = special.z - self.z;

        if delta > self.height || delta < -8.0 {
            // Can't reach it. Because map is essentially 2D we need to check Z
            return;
        }

        let mut sound = SfxEnum::itemup;

        if let Some(player) = self.player {
            let player = unsafe { &mut *player };

            if self.health <= 0 {
                // dead thing, like a gib or corpse
                return;
            }

            let skill = unsafe { (*self.level).game_skill };
            match special.sprite {
                SpriteNum::SPR_ARM1 => {
                    if !player.give_armour(1) {
                        return;
                    }
                    player.message = Some(GOTARMOR);
                }
                SpriteNum::SPR_ARM2 => {
                    if !player.give_armour(2) {
                        return;
                    }
                    player.message = Some(GOTMEGA);
                }
                SpriteNum::SPR_BON1 => {
                    player.health += 1; // Go over 100%
                    if player.health > 200 {
                        player.health = 200;
                    }
                    player.message = Some(GOTHTHBONUS);
                }
                SpriteNum::SPR_BON2 => {
                    player.armorpoints += 1; // Go over 100%
                    if player.armorpoints > 200 {
                        player.armorpoints = 200;
                    }
                    if player.armortype == 0 {
                        player.armortype = 1;
                    }
                    player.message = Some(GOTARMBONUS);
                }
                SpriteNum::SPR_SOUL => {
                    player.health += 100;
                    if player.health > 200 {
                        player.health = 200;
                    }
                    player.message = Some(GOTSUPER);
                    sound = SfxEnum::getpow;
                }
                SpriteNum::SPR_MEGA => {
                    // TODO: if (gamemode != commercial) return;
                    player.health = 200;
                    player.give_armour(2);
                    player.message = Some(GOTMSPHERE);
                    sound = SfxEnum::getpow;
                }

                // Keycards
                SpriteNum::SPR_BKEY => {
                    if !player.cards[Card::Bluecard as usize] {
                        player.message = Some(GOTBLUECARD);
                    }
                    player.give_key(Card::Bluecard);
                    // TODO: if (netgame) return;
                }
                SpriteNum::SPR_YKEY => {
                    if !player.cards[Card::Yellowcard as usize] {
                        player.message = Some(GOTYELWCARD);
                    }
                    player.give_key(Card::Yellowcard);
                    // TODO: if (netgame) return;
                }
                SpriteNum::SPR_RKEY => {
                    if !player.cards[Card::Redcard as usize] {
                        player.message = Some(GOTREDCARD);
                    }
                    player.give_key(Card::Redcard);
                    // TODO: if (netgame) return;
                }
                SpriteNum::SPR_BSKU => {
                    if !player.cards[Card::Blueskull as usize] {
                        player.message = Some(GOTBLUESKUL);
                    }
                    player.give_key(Card::Blueskull);
                    // TODO: if (netgame) return;
                }
                SpriteNum::SPR_YSKU => {
                    if !player.cards[Card::Yellowskull as usize] {
                        player.message = Some(GOTYELWSKUL);
                    }
                    player.give_key(Card::Yellowskull);
                    // TODO: if (netgame) return;
                }
                SpriteNum::SPR_RSKU => {
                    if !player.cards[Card::Redskull as usize] {
                        player.message = Some(GOTREDSKULL);
                    }
                    player.give_key(Card::Redskull);
                    // TODO: if (netgame) return;
                }
                SpriteNum::SPR_STIM => {
                    if !player.give_body(10) {
                        return;
                    }
                    player.message = Some(GOTSTIM);
                }
                SpriteNum::SPR_MEDI => {
                    if !player.give_body(25) {
                        return;
                    }
                    if player.health < 25 {
                        player.message = Some(GOTMEDINEED);
                    } else {
                        player.message = Some(GOTMEDIKIT);
                    }
                }

                // Powerups
                SpriteNum::SPR_PINV => {
                    if !player.give_power(PowerType::Invulnerability) {
                        return;
                    }
                    player.message = Some(GOTINVUL);
                    // TODO: sound = sfx_getpow;
                }
                SpriteNum::SPR_PSTR => {
                    if !player.give_power(PowerType::Strength) {
                        return;
                    }
                    player.message = Some(GOTBERSERK);
                    if !(player.readyweapon == WeaponType::Fist) {
                        player.pendingweapon = WeaponType::Fist;
                    }
                    // TODO: sound = sfx_getpow;
                }
                SpriteNum::SPR_PINS => {
                    if !player.give_power(PowerType::Invisibility) {
                        return;
                    }
                    self.flags |= MapObjectFlag::Shadow as u32;
                    player.message = Some(GOTINVIS);
                    // TODO: sound = sfx_getpow;
                }
                SpriteNum::SPR_SUIT => {
                    if !player.give_power(PowerType::IronFeet) {
                        return;
                    }
                    player.message = Some(GOTSUIT);
                    // TODO: sound = sfx_getpow;
                }
                SpriteNum::SPR_PMAP => {
                    if !player.give_power(PowerType::Allmap) {
                        return;
                    }
                    player.message = Some(GOTMAP);
                    // TODO: sound = sfx_getpow;
                }
                SpriteNum::SPR_PVIS => {
                    if !player.give_power(PowerType::Infrared) {
                        return;
                    }
                    player.message = Some(GOTVISOR);
                    // TODO: sound = sfx_getpow;
                }

                // Ammo
                SpriteNum::SPR_CLIP => {
                    if (special.flags & MapObjectFlag::Dropped as u32 != 0
                        && !player.give_ammo(AmmoType::Clip, 0, skill))
                        || !player.give_ammo(AmmoType::Clip, 1, skill)
                    {
                        return;
                    }
                    player.message = Some(GOTCLIP);
                }
                SpriteNum::SPR_AMMO => {
                    if !player.give_ammo(AmmoType::Clip, 5, skill) {
                        return;
                    }
                    player.message = Some(GOTCLIPBOX);
                }
                SpriteNum::SPR_ROCK => {
                    if !player.give_ammo(AmmoType::Missile, 1, skill) {
                        return;
                    }
                    player.message = Some(GOTROCKET);
                }
                SpriteNum::SPR_BROK => {
                    if !player.give_ammo(AmmoType::Missile, 5, skill) {
                        return;
                    }
                    player.message = Some(GOTROCKBOX);
                }
                SpriteNum::SPR_CELL => {
                    if !player.give_ammo(AmmoType::Cell, 1, skill) {
                        return;
                    }
                    player.message = Some(GOTCELL);
                }
                SpriteNum::SPR_CELP => {
                    if !player.give_ammo(AmmoType::Cell, 5, skill) {
                        return;
                    }
                    player.message = Some(GOTCELLBOX);
                }
                SpriteNum::SPR_SHEL => {
                    if !player.give_ammo(AmmoType::Shell, 1, skill) {
                        return;
                    }
                    player.message = Some(GOTSHELLS);
                }
                SpriteNum::SPR_SBOX => {
                    if !player.give_ammo(AmmoType::Shell, 5, skill) {
                        return;
                    }
                    player.message = Some(GOTSHELLBOX);
                }
                SpriteNum::SPR_BPAK => {
                    if !player.backpack {
                        for i in 0..AmmoType::NumAmmo as usize {
                            player.maxammo[i] *= 2;
                        }
                        player.backpack = true;
                    }
                    for i in 0..AmmoType::NumAmmo as usize {
                        player.give_ammo(AmmoType::from(i), 1, skill);
                    }
                    player.message = Some(GOTBACKPACK);
                }

                // Weapons
                SpriteNum::SPR_BFUG => {
                    if !player.give_weapon(WeaponType::BFG, false, skill) {
                        return;
                    }
                    player.message = Some(GOTBFG9000);
                    sound = SfxEnum::wpnup;
                }
                SpriteNum::SPR_MGUN => {
                    if !player.give_weapon(
                        WeaponType::Chaingun,
                        special.flags & MapObjectFlag::Dropped as u32 != 0,
                        skill,
                    ) {
                        return;
                    }
                    player.message = Some(GOTCHAINGUN);
                    sound = SfxEnum::wpnup;
                }
                SpriteNum::SPR_CSAW => {
                    if !player.give_weapon(WeaponType::Chainsaw, false, skill) {
                        return;
                    }
                    player.message = Some(GOTCHAINSAW);
                    sound = SfxEnum::wpnup;
                }
                SpriteNum::SPR_LAUN => {
                    if !player.give_weapon(WeaponType::Missile, false, skill) {
                        return;
                    }
                    player.message = Some(GOTLAUNCHER);
                    sound = SfxEnum::wpnup;
                }
                SpriteNum::SPR_PLAS => {
                    if !player.give_weapon(WeaponType::Plasma, false, skill) {
                        return;
                    }
                    player.message = Some(GOTPLASMA);
                    sound = SfxEnum::wpnup;
                }
                SpriteNum::SPR_SHOT => {
                    if !player.give_weapon(
                        WeaponType::Shotgun,
                        special.flags & MapObjectFlag::Dropped as u32 != 0,
                        skill,
                    ) {
                        return;
                    }
                    player.message = Some(GOTSHOTGUN);
                    sound = SfxEnum::wpnup;
                }
                SpriteNum::SPR_SGN2 => {
                    if !player.give_weapon(
                        WeaponType::SuperShotgun,
                        special.flags & MapObjectFlag::Dropped as u32 != 0,
                        skill,
                    ) {
                        return;
                    }
                    player.message = Some(GOTSHOTGUN2);
                    sound = SfxEnum::wpnup;
                }

                _ => error!("Unknown gettable: {:?}", special.sprite),
            }

            // Ensure mobj health is synced
            self.health = player.health;

            if special.flags & MapObjectFlag::CountItem as u32 != 0 {
                player.itemcount += 1;
            }
            special.remove();
            player.bonuscount += BONUSADD;

            // TODO: if (player == &players[consoleplayer])
            self.start_sound(sound);
        }
    }
}

const BASETHRESHOLD: i32 = 100;
