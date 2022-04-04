//! Doom source name `p_inter`

use std::ptr;

use glam::Vec2;
use log::{debug, error, info};

use super::{
    map_object::{MapObject, MobjFlag},
    utilities::p_random,
};
use crate::{
    d_main::Skill,
    doom_def::{PowerType, WeaponType},
    info::{MapObjectType, StateNum, STATES},
    play::{
        player::{PlayerCheat, PlayerState},
        utilities::point_to_angle_2,
    },
};

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
    pub fn p_take_damage(
        &mut self,
        inflictor: Option<&MapObject>,
        mut source: Option<&mut MapObject>,
        source_is_inflictor: bool,
        mut damage: i32,
    ) {
        if self.flags & MobjFlag::SHOOTABLE as u32 == 0 {
            return;
        }

        if self.health <= 0 {
            return;
        }

        if self.flags & MobjFlag::SKULLFLY as u32 != 0 {
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
            let inflict = if source_is_inflictor {
                // assume source is not None
                // DOn't push away if it's a player with a chainsaw
                do_push = source.as_ref().unwrap().player.is_none()
                    || unsafe {
                        (*source.as_ref().unwrap().player.unwrap()).readyweapon
                            != WeaponType::Chainsaw
                    };
                source.as_mut().unwrap()
            } else {
                inflictor.unwrap()
            };

            if self.flags & MobjFlag::NOCLIP as u32 == 0 && do_push {
                let mut angle = point_to_angle_2(&inflict.xy, &self.xy);
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

        if p_random() < self.info.painchance && self.flags & MobjFlag::SKULLFLY as u32 == 0 {
            self.flags |= MobjFlag::JUSTHIT as u32; // FIGHT!!!
            self.set_state(self.info.painstate);
        }

        self.reactiontime = 0; // AWAKE AND READY!

        if self.threshold == 0 || self.kind == MapObjectType::MT_VILE {
            if let Some(source) = source {
                // TODO: gameversion <= exe_doom_1_2
                if !ptr::eq(self, source) && source.kind != MapObjectType::MT_VILE {
                    self.target = Some(source);
                    self.threshold = BASETHRESHOLD;

                    if std::ptr::eq(self.state, &STATES[self.info.spawnstate as usize]) {
                        self.set_state(self.info.seestate);
                    }
                }
            }
        }
    }

    /// Doom function name `P_KillMobj`
    fn kill(&mut self, mut source: Option<&mut MapObject>) {
        self.flags &=
            !(MobjFlag::SHOOTABLE as u32 | MobjFlag::FLOAT as u32 | MobjFlag::SKULLFLY as u32);

        if self.kind != MapObjectType::MT_SKULL {
            self.flags &= !(MobjFlag::NOGRAVITY as u32);
        }

        self.flags |= MobjFlag::CORPSE as u32 | MobjFlag::DROPOFF as u32;
        self.health >>= 2;

        if let Some(source) = source.as_ref() {
            if let Some(player) = source.player {
                if self.flags & MobjFlag::COUNTKILL as u32 != 0 {
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

                self.flags &= !(MobjFlag::SOLID as u32);
                player.player_state = PlayerState::PstDead;
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
                self.xy.x(),
                self.xy.y(),
                self.floorz as i32,
                item,
                &mut *self.level,
            );
            (*mobj).flags |= MobjFlag::DROPPED as u32;
        }
    }
}

const BASETHRESHOLD: i32 = 100;
