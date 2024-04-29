//! Environment and object interactions

use std::ptr;

use glam::Vec2;
use log::{debug, error, info};
use sound_traits::SfxName;

use crate::{
    doom_def::{AmmoType, Card, PowerType, WeaponType},
    info::{MapObjKind, SpriteNum, StateNum, STATES},
    lang::english::*,
    player::{PlayerCheat, PlayerState},
    thing::MapObjFlag,
    utilities::{p_random, point_to_angle_2},
    MapObject, Skill,
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
        if self.flags & MapObjFlag::Shootable as u32 == 0 {
            return;
        }

        if self.health <= 0 {
            return;
        }

        if self.flags & MapObjFlag::Skullfly as u32 != 0 {
            self.momxy = Vec2::default();
            self.momz = 0.0;
            // extra flag setting here because sometimes float errors stuff it up
            self.flags &= !(MapObjFlag::Skullfly as u32);
            self.set_state(self.info.spawnstate);
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
                        (*source.as_ref().unwrap().player.unwrap())
                            .status
                            .readyweapon
                            != WeaponType::Chainsaw
                    };
                source.as_mut().unwrap()
            };

            if self.flags & MapObjFlag::Noclip as u32 == 0 && do_push {
                let angle = point_to_angle_2(self.xy, inflict.xy);
                let mut thrust = damage as f32 * 16.66 / self.info.mass as f32;
                // make fall forwards sometimes
                if damage < 40
                    && damage > self.health
                    && self.z - inflict.z > 64.0
                    && p_random() & 1 != 0
                {
                    thrust *= 4.0;
                }

                self.momxy += angle.unit() * thrust;
            }
        }

        let special = unsafe { (*self.subsector).sector.special };
        let mobj_health = self.health;
        let self_pos = self.xy;
        let self_ang = self.angle;
        if let Some(player) = self.player_mut() {
            // end of game-exe hell hack
            if special == 11 && damage >= mobj_health {
                damage = mobj_health - 1;
            }
            // Below certain threshold, ignore damage in GOD mode, or with INVUL power.
            if damage < 1000
                && (player.status.cheats & PlayerCheat::Godmode as u32 != 0
                    || player.powers[PowerType::Invulnerability as usize] != 0)
            {
                return;
            }

            if player.status.armortype != 0 {
                let mut saved = if player.status.armortype == 1 {
                    damage / 3
                } else {
                    damage / 2
                };

                if player.status.armorpoints <= saved {
                    // armour is used up
                    saved = player.status.armorpoints;
                    player.status.armortype = 0;
                }
                player.status.armorpoints -= saved;
                damage -= saved;
            }

            player.status.health -= damage;
            if player.status.health < 0 {
                player.status.health = 0;
            }

            if let Some(source) = source.as_mut() {
                player.status.attacked_from = point_to_angle_2(self_pos, source.xy);
                player.status.own_angle = self_ang;
                player.status.attacked_angle_count = 6;
                player.attacker = Some(*source);
            }

            player.status.damagecount += damage;
            if player.status.damagecount > 100 {
                player.status.damagecount = 100; // teleport stomp does 10k points...
            }
            // Tactile feedback thing removed here
        }

        debug!("Applying {damage} damage");
        self.health -= damage;
        if self.health <= 0 {
            self.kill(source);
            return;
        }

        if p_random() < self.info.painchance && self.flags & MapObjFlag::Skullfly as u32 == 0 {
            self.flags |= MapObjFlag::Justhit as u32; // FIGHT!!!
            self.set_state(self.info.painstate);
        }

        self.reactiontime = 0; // AWAKE AND READY!

        if self.threshold == 0 || self.kind == MapObjKind::MT_VILE {
            if let Some(source) = source {
                // TODO: gameversion <= exe_doom_1_2
                if !ptr::eq(self, source) && source.kind != MapObjKind::MT_VILE {
                    self.target = Some(source.thinker);
                    self.threshold = BASETHRESHOLD;

                    if ptr::eq(self.state, &STATES[self.info.spawnstate as usize])
                        && self.info.seestate != StateNum::None
                    {
                        self.set_state(self.info.seestate);
                    }
                }
            }
        }
    }

    /// Doom function name `P_KillMobj`
    fn kill(&mut self, mut source: Option<&mut MapObject>) {
        self.flags &= !(MapObjFlag::Shootable as u32
            | MapObjFlag::Float as u32
            | MapObjFlag::Skullfly as u32);

        if self.kind != MapObjKind::MT_SKULL {
            self.flags &= !(MapObjFlag::Nogravity as u32);
        }

        self.flags |= MapObjFlag::Corpse as u32 | MapObjFlag::Dropoff as u32;
        self.height /= 4.0;

        if let Some(source) = source.as_mut() {
            if let Some(player) = source.player_mut() {
                if self.flags & MapObjFlag::Countkill as u32 != 0 {
                    player.killcount += 1;
                }

                if self.player.is_some() {
                    // TODO: set correct player for frags
                    player.frags[0] += 1;
                }
            }
        } else {
            // TODO: Need to increment killcount for first player
            //players[0].killcount++;
        }

        if let Some(player) = self.player_mut() {
            info!("Killing player");
            // Environment kills count against you
            if source.is_none() {
                // TODO: set correct player for frags
                player.frags[0] += 1;
            }

            player.player_state = PlayerState::Dead;
            // TODO: P_DropWeapon(target->player);
            error!("P_DropWeapon not implemented");
            // TODO: stop automap
        }

        if self.player().is_some() {
            self.flags &= !(MapObjFlag::Solid as u32);
        }

        if self.health < -self.info.spawnhealth && self.info.xdeathstate != StateNum::None {
            self.set_state(self.info.xdeathstate);
        } else {
            self.set_state(self.info.deathstate);
        }

        self.tics -= p_random() & 3;
        if self.tics < 1 {
            self.tics = 1;
        }

        let item = match self.kind {
            MapObjKind::MT_WOLFSS | MapObjKind::MT_POSSESSED => MapObjKind::MT_CLIP,
            MapObjKind::MT_SHOTGUY => MapObjKind::MT_SHOTGUN,
            MapObjKind::MT_CHAINGUY => MapObjKind::MT_CHAINGUN,
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
            (*mobj).flags |= MapObjFlag::Dropped as u32;
        }
    }

    /// Interact with special pickups
    ///
    /// Doom function name `P_TouchSpecialThing`
    pub(crate) fn touch_special(&mut self, special: &mut MapObject) {
        let delta = special.z - self.z;

        if delta > self.height || delta < -8.0 {
            // Can't reach it. Because map is essentially 2D we need to check Z
            return;
        }

        let mut sound = SfxName::Itemup;

        if let Some(player) = self.player {
            let player = unsafe { &mut *player };

            if self.health <= 0 {
                // dead thing, like a gib or corpse
                return;
            }

            let skill = unsafe { (*self.level).game_skill };
            match special.sprite {
                SpriteNum::ARM1 => {
                    if !player.give_armour(1) {
                        return;
                    }
                    player.message = Some(GOTARMOR);
                }
                SpriteNum::ARM2 => {
                    if !player.give_armour(2) {
                        return;
                    }
                    player.message = Some(GOTMEGA);
                }
                SpriteNum::BON1 => {
                    player.status.health += 1; // Go over 100%
                    if player.status.health > 200 {
                        player.status.health = 200;
                    }
                    player.message = Some(GOTHTHBONUS);
                }
                SpriteNum::BON2 => {
                    player.status.armorpoints += 1; // Go over 100%
                    if player.status.armorpoints > 200 {
                        player.status.armorpoints = 200;
                    }
                    if player.status.armortype == 0 {
                        player.status.armortype = 1;
                    }
                    player.message = Some(GOTARMBONUS);
                }
                SpriteNum::SOUL => {
                    player.status.health += 100;
                    if player.status.health > 200 {
                        player.status.health = 200;
                    }
                    player.message = Some(GOTSUPER);
                    sound = SfxName::Getpow;
                }
                SpriteNum::MEGA => {
                    // TODO: if (gamemode != commercial) return;
                    player.status.health = 200;
                    player.give_armour(2);
                    player.message = Some(GOTMSPHERE);
                    sound = SfxName::Getpow;
                }

                // Keycards
                SpriteNum::BKEY => {
                    if !player.status.cards[Card::Bluecard as usize] {
                        player.message = Some(GOTBLUECARD);
                    }
                    player.give_key(Card::Bluecard);
                    // TODO: if (netgame) return;
                }
                SpriteNum::YKEY => {
                    if !player.status.cards[Card::Yellowcard as usize] {
                        player.message = Some(GOTYELWCARD);
                    }
                    player.give_key(Card::Yellowcard);
                    // TODO: if (netgame) return;
                }
                SpriteNum::RKEY => {
                    if !player.status.cards[Card::Redcard as usize] {
                        player.message = Some(GOTREDCARD);
                    }
                    player.give_key(Card::Redcard);
                    // TODO: if (netgame) return;
                }
                SpriteNum::BSKU => {
                    if !player.status.cards[Card::Blueskull as usize] {
                        player.message = Some(GOTBLUESKUL);
                    }
                    player.give_key(Card::Blueskull);
                    // TODO: if (netgame) return;
                }
                SpriteNum::YSKU => {
                    if !player.status.cards[Card::Yellowskull as usize] {
                        player.message = Some(GOTYELWSKUL);
                    }
                    player.give_key(Card::Yellowskull);
                    // TODO: if (netgame) return;
                }
                SpriteNum::RSKU => {
                    if !player.status.cards[Card::Redskull as usize] {
                        player.message = Some(GOTREDSKULL);
                    }
                    player.give_key(Card::Redskull);
                    // TODO: if (netgame) return;
                }
                SpriteNum::STIM => {
                    if !player.give_body(10) {
                        return;
                    }
                    player.message = Some(GOTSTIM);
                }
                SpriteNum::MEDI => {
                    if !player.give_body(25) {
                        return;
                    }
                    if player.status.health < 25 {
                        player.message = Some(GOTMEDINEED);
                    } else {
                        player.message = Some(GOTMEDIKIT);
                    }
                }

                // Powerups
                SpriteNum::PINV => {
                    if !player.give_power(PowerType::Invulnerability) {
                        return;
                    }
                    player.message = Some(GOTINVUL);
                    // TODO: sound = sfx_getpow;
                }
                SpriteNum::PSTR => {
                    if !player.give_power(PowerType::Strength) {
                        return;
                    }
                    player.message = Some(GOTBERSERK);
                    if !(player.status.readyweapon == WeaponType::Fist) {
                        player.pendingweapon = WeaponType::Fist;
                    }
                    // TODO: sound = sfx_getpow;
                }
                SpriteNum::PINS => {
                    if !player.give_power(PowerType::Invisibility) {
                        return;
                    }
                    self.flags |= MapObjFlag::Shadow as u32;
                    player.message = Some(GOTINVIS);
                    // TODO: sound = sfx_getpow;
                }
                SpriteNum::SUIT => {
                    if !player.give_power(PowerType::IronFeet) {
                        return;
                    }
                    player.message = Some(GOTSUIT);
                    // TODO: sound = sfx_getpow;
                }
                SpriteNum::PMAP => {
                    if !player.give_power(PowerType::Allmap) {
                        return;
                    }
                    player.message = Some(GOTMAP);
                    // TODO: sound = sfx_getpow;
                }
                SpriteNum::PVIS => {
                    if !player.give_power(PowerType::Infrared) {
                        return;
                    }
                    player.message = Some(GOTVISOR);
                    // TODO: sound = sfx_getpow;
                }

                // Ammo
                SpriteNum::CLIP => {
                    if (special.flags & MapObjFlag::Dropped as u32 != 0
                        && !player.give_ammo(AmmoType::Clip, 0, skill))
                        || !player.give_ammo(AmmoType::Clip, 1, skill)
                    {
                        return;
                    }
                    player.message = Some(GOTCLIP);
                }
                SpriteNum::AMMO => {
                    if !player.give_ammo(AmmoType::Clip, 5, skill) {
                        return;
                    }
                    player.message = Some(GOTCLIPBOX);
                }
                SpriteNum::ROCK => {
                    if !player.give_ammo(AmmoType::Missile, 1, skill) {
                        return;
                    }
                    player.message = Some(GOTROCKET);
                }
                SpriteNum::BROK => {
                    if !player.give_ammo(AmmoType::Missile, 5, skill) {
                        return;
                    }
                    player.message = Some(GOTROCKBOX);
                }
                SpriteNum::CELL => {
                    if !player.give_ammo(AmmoType::Cell, 1, skill) {
                        return;
                    }
                    player.message = Some(GOTCELL);
                }
                SpriteNum::CELP => {
                    if !player.give_ammo(AmmoType::Cell, 5, skill) {
                        return;
                    }
                    player.message = Some(GOTCELLBOX);
                }
                SpriteNum::SHEL => {
                    if !player.give_ammo(AmmoType::Shell, 1, skill) {
                        return;
                    }
                    player.message = Some(GOTSHELLS);
                }
                SpriteNum::SBOX => {
                    if !player.give_ammo(AmmoType::Shell, 5, skill) {
                        return;
                    }
                    player.message = Some(GOTSHELLBOX);
                }
                SpriteNum::BPAK => {
                    if !player.status.backpack {
                        for i in 0..AmmoType::NumAmmo as usize {
                            player.status.maxammo[i] *= 2;
                        }
                        player.status.backpack = true;
                    }
                    for i in 0..AmmoType::NumAmmo as usize {
                        player.give_ammo(AmmoType::from(i), 1, skill);
                    }
                    player.message = Some(GOTBACKPACK);
                }

                // Weapons
                SpriteNum::BFUG => {
                    if !player.give_weapon(WeaponType::BFG, false, skill) {
                        return;
                    }
                    player.message = Some(GOTBFG9000);
                    sound = SfxName::Wpnup;
                }
                SpriteNum::MGUN => {
                    if !player.give_weapon(
                        WeaponType::Chaingun,
                        special.flags & MapObjFlag::Dropped as u32 != 0,
                        skill,
                    ) {
                        return;
                    }
                    player.message = Some(GOTCHAINGUN);
                    sound = SfxName::Wpnup;
                }
                SpriteNum::CSAW => {
                    if !player.give_weapon(WeaponType::Chainsaw, false, skill) {
                        return;
                    }
                    player.message = Some(GOTCHAINSAW);
                    sound = SfxName::Wpnup;
                }
                SpriteNum::LAUN => {
                    if !player.give_weapon(WeaponType::Missile, false, skill) {
                        return;
                    }
                    player.message = Some(GOTLAUNCHER);
                    sound = SfxName::Wpnup;
                }
                SpriteNum::PLAS => {
                    if !player.give_weapon(WeaponType::Plasma, false, skill) {
                        return;
                    }
                    player.message = Some(GOTPLASMA);
                    sound = SfxName::Wpnup;
                }
                SpriteNum::SHOT => {
                    if !player.give_weapon(
                        WeaponType::Shotgun,
                        special.flags & MapObjFlag::Dropped as u32 != 0,
                        skill,
                    ) {
                        return;
                    }
                    player.message = Some(GOTSHOTGUN);
                    sound = SfxName::Wpnup;
                }
                SpriteNum::SGN2 => {
                    if !player.give_weapon(
                        WeaponType::SuperShotgun,
                        special.flags & MapObjFlag::Dropped as u32 != 0,
                        skill,
                    ) {
                        return;
                    }
                    player.message = Some(GOTSHOTGUN2);
                    sound = SfxName::Wpnup;
                }

                _ => error!("Unknown gettable: {:?}", special.sprite),
            }

            // Ensure thing health is synced
            self.health = player.status.health;

            if special.flags & MapObjFlag::Countitem as u32 != 0 {
                player.itemcount += 1;
            }
            special.remove();
            player.status.bonuscount += BONUSADD;

            // TODO: if (player == &players[consoleplayer])
            self.start_sound(sound);
        }
    }
}

const BASETHRESHOLD: i32 = 100;
