//! Doom source name `p_inter`

use glam::Vec2;
use log::{debug, info};

use super::{
    map_object::{MapObject, MobjFlag},
    utilities::p_random,
};
use crate::{
    d_main::Skill,
    doom_def::{PowerType, WeaponType},
    info::{MapObjectType, STATES},
    play::{
        player::{PlayerCheat, PlayerState},
        utilities::point_to_angle_2,
    },
};

impl MapObject {
    /// Doom function name `P_DamageMobj`
    pub fn p_take_damage(
        &mut self,
        inflictor: Option<&MapObject>,
        mut source: Option<&mut MapObject>,
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
        if let Some(inflictor) = inflictor {
            if self.flags & MobjFlag::NOCLIP as u32 == 0
                && (source.is_none()
                    || source.as_ref().unwrap().player.is_none()
                    || unsafe {
                        source
                            .as_ref()
                            .unwrap()
                            .player
                            .unwrap()
                            .as_ref()
                            .readyweapon
                            != WeaponType::wp_chainsaw
                    })
            {
                let mut angle = point_to_angle_2(&inflictor.xy, &self.xy);
                let mut thrust = damage as f32 * 0.001 * 100.0 / self.info.mass as f32;
                // make fall forwards sometimes
                if damage < 40
                    && damage > self.health
                    && self.z - inflictor.z > 64.0
                    && p_random() & 1 != 0
                {
                    angle += 180.0; // TODO: verify the results of this (fixed vs float)
                    thrust *= 4.0;
                }

                self.momxy += angle.unit() * thrust;
            }
        }

        if let Some(mut player) = self.player {
            info!("Ouch!");
            unsafe {
                let mut player = player.as_mut();

                // end of game hell hack
                if (*self.subsector).sector.special == 11 && damage >= self.health {
                    damage = self.health - 1;
                }
                // Below certain threshold, ignore damage in GOD mode, or with INVUL power.
                if damage < 1000
                    && (player.cheats & PlayerCheat::Godmode as u32 != 0
                        || player.powers[PowerType::pw_invulnerability as usize] != 0)
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

        self.health -= damage;
        if self.health <= 0 {
            // TODO: P_KillMobj(source, target);
            if let Some(player) = self.player.as_mut() {
                info!("Killing player");
                unsafe {
                    let mut player = player.as_mut();
                    player.player_state = PlayerState::PstDead;
                }
            }
            return;
        }

        if p_random() < self.info.painchance && self.flags & MobjFlag::SKULLFLY as u32 == 0 {
            self.flags |= MobjFlag::JUSTHIT as u32; // FIGHT!!!
            self.set_state(self.info.painstate);
        }

        self.reactiontime = 0; // AWAKE AND READY!

        // TODO: finish this part
        if self.threshold == 0 || self.kind == MapObjectType::MT_VILE {
            if let Some(source) = source {
                if source.kind != MapObjectType::MT_VILE {
                    self.target = Some(source);
                    self.threshold = BASETHRESHOLD;

                    if std::ptr::eq(self.state, &STATES[self.info.spawnstate as usize]) {
                        self.set_state(self.info.seestate);
                    }
                }
            }
        }
    }
}

const BASETHRESHOLD: i32 = 100;
