use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use crate::PlayerStatus;
use gamestate_traits::{m_random, WeaponType, TICRATE};
use wad::{
    lumps::{WadPatch, WAD_PATCH},
    WadData,
};

const PAIN_FACES: usize = 5;
const STRAIGHT_FACES: usize = 3;
const TURN_FACES: usize = 2;
const SPECIAL_FACES: usize = 3;
const EXTRA_FACES: usize = 3;

const FACE_STRIDE: usize = STRAIGHT_FACES + TURN_FACES + SPECIAL_FACES;
const FACE_COUNT: usize = FACE_STRIDE * PAIN_FACES + EXTRA_FACES;

const TURN_OFFSET: usize = STRAIGHT_FACES;
const OUCH_OFFSET: usize = TURN_OFFSET + TURN_FACES;
const EVIL_OFFSET: usize = OUCH_OFFSET + 1;
const RAMPAGE_OFFSET: usize = EVIL_OFFSET + 1;
const IMMORTAL_FACE: usize = PAIN_FACES * FACE_STRIDE;
const DEAD_FACE: usize = IMMORTAL_FACE + 1;

const EVIL_TICS: usize = 2 * TICRATE as usize;
const STRAIGHT_TICS: usize = TICRATE as usize / 2;
const TURN_TICS: usize = TICRATE as usize;
const RAMPAGE_DELAY: i32 = 2 * TICRATE;

const MUCH_PAIN: i32 = 20;

pub(crate) struct DoomguyFace {
    faces: [WadPatch; FACE_COUNT],
    /// Index to the face to show
    index: usize,
    /// How many to show
    count: usize,
    //
    old_weapons_owned: [bool; WeaponType::NumWeapons as usize],
    old_health: i32,
    last_pain_calc: i32,
    last_attack_down: i32,
    rand: i32,
    priority: i32,
}

impl DoomguyFace {
    pub(crate) fn new(wad: &WadData) -> Self {
        let mut face_num = 0;
        let mut faces: [WadPatch; FACE_COUNT] = [WAD_PATCH; FACE_COUNT];
        for p in 0..PAIN_FACES {
            for s in 0..STRAIGHT_FACES {
                let lump = wad.get_lump(&format!("STFST{p}{s}")).unwrap();
                faces[face_num] = WadPatch::from_lump(lump);
                face_num += 1;
            }
            // turn right
            let lump = wad.get_lump(&format!("STFTR{p}0")).unwrap();
            faces[face_num] = WadPatch::from_lump(lump);
            face_num += 1;
            // turn left
            let lump = wad.get_lump(&format!("STFTL{p}0")).unwrap();
            faces[face_num] = WadPatch::from_lump(lump);
            face_num += 1;
            // ouch
            let lump = wad.get_lump(&format!("STFOUCH{p}")).unwrap();
            faces[face_num] = WadPatch::from_lump(lump);
            face_num += 1;
            // evil
            let lump = wad.get_lump(&format!("STFEVL{p}")).unwrap();
            faces[face_num] = WadPatch::from_lump(lump);
            face_num += 1;
            // kill
            let lump = wad.get_lump(&format!("STFKILL{p}")).unwrap();
            faces[face_num] = WadPatch::from_lump(lump);
            face_num += 1;
        }
        // immortal
        let lump = wad.get_lump("STFGOD0").unwrap();
        faces[face_num] = WadPatch::from_lump(lump);
        face_num += 1;
        // dead
        let lump = wad.get_lump("STFDEAD0").unwrap();
        faces[face_num] = WadPatch::from_lump(lump);

        Self {
            faces,
            index: 0,
            count: 0,
            old_weapons_owned: Default::default(),
            old_health: -1,
            last_pain_calc: 0,
            last_attack_down: -1,
            rand: 0,
            priority: 0,
        }
    }

    pub(crate) fn tick(&mut self, status: &PlayerStatus) {
        self.rand = m_random();
        self.update_face(status);
        //self.old_health = status.health;
        self.old_weapons_owned = status.weaponowned;
    }

    fn calc_pain_offset(&mut self, status: &PlayerStatus) -> usize {
        let health = if status.health > 100 {
            100
        } else {
            status.health
        };

        if health != self.old_health {
            self.last_pain_calc = FACE_STRIDE as i32 * (((100 - health) * PAIN_FACES as i32) / 101);
            self.old_health = health;
        }

        if self.last_pain_calc < 0 {
            self.last_pain_calc = 0;
        }

        self.last_pain_calc as usize
    }

    fn update_face(&mut self, status: &PlayerStatus) {
        if self.priority < 10 {
            // dead
            if status.health <= 0 {
                self.priority = 9;
                self.index = DEAD_FACE;
                self.count = 1;
            }
        }

        if self.priority < 9 && status.bonuscount != 0 {
            // picking up bonus
            let mut doevilgrin = false;

            for (i, w) in status.weaponowned.iter().enumerate() {
                if self.old_weapons_owned[i] != *w {
                    doevilgrin = true;
                    self.old_weapons_owned[i] = *w;
                }
            }
            if doevilgrin {
                // evil grin if just picked up weapon
                self.priority = 8;
                self.count = EVIL_TICS;
                self.index = self.calc_pain_offset(status) + EVIL_OFFSET;
            }
        }

        // being attacked
        if self.priority < 8 {
            if status.damagecount != 0 {
                self.priority = 7;
                if self.old_health - status.health >= MUCH_PAIN {
                    self.count = TURN_TICS;
                    self.index = self.calc_pain_offset(status) + OUCH_OFFSET;
                } else {
                    // TODO: else show angle
                    // if status.attacked_angle_count != 0 {}
                    let i;
                    let diffang;
                    if status.attacked_from.rad() > status.own_angle.rad() {
                        // whether right or left
                        diffang = status.attacked_from.rad() - status.own_angle.rad();
                        i = diffang > PI;
                    } else {
                        // whether left or right
                        diffang = status.own_angle.rad() - status.attacked_from.rad();
                        i = diffang <= PI;
                    }

                    self.count = TURN_TICS;
                    self.index = self.calc_pain_offset(status);

                    if diffang > PI - FRAC_PI_4 {
                        // head-on
                        self.index += RAMPAGE_OFFSET;
                    } else if i {
                        // turn face right
                        self.index += TURN_OFFSET + 1;
                    } else {
                        // turn face left
                        self.index += TURN_OFFSET;
                    }
                }
            }
        }

        if self.priority < 7 {
            // getting hurt because of your own damn stupidity
            if status.damagecount != 0 {
                if self.old_health - status.health >= MUCH_PAIN {
                    self.priority = 7;
                    self.count = TURN_TICS;
                    self.index = self.calc_pain_offset(status) + OUCH_OFFSET;
                } else {
                    self.priority = 6;
                    self.count = TURN_TICS;
                    self.index = self.calc_pain_offset(status) + RAMPAGE_OFFSET;
                }
            }
        }

        if self.priority < 6 {
            // rapid firing
            if status.attackdown {
                if self.last_attack_down == -1 {
                    self.last_attack_down = RAMPAGE_DELAY;
                } else {
                    self.last_attack_down -= 1;
                    if self.last_attack_down == 0 {
                        self.priority = 5;
                        self.index = self.calc_pain_offset(status) + RAMPAGE_OFFSET;
                        self.count = 1;
                        self.last_attack_down = 1;
                    }
                }
            } else {
                self.last_attack_down = -1;
            }
        }

        // if (self.priority < 5) {
        //     // TODO invulnerability
        //     if (status.cheats & CF_GODMODE) || plyr->powers[pw_invulnerability]) {
        //         self.priority = 4;

        //         self.index = ST_GODFACE;
        //         self.count = 1;
        //     }
        // }

        // look left or look right if the facecount has timed out
        if self.count == 0 {
            self.index = self.calc_pain_offset(status) + (self.rand % 3) as usize;
            self.count = STRAIGHT_TICS;
            self.priority = 0;
        }

        self.count -= 1;
    }

    pub(crate) fn get_face(&self) -> &WadPatch {
        &self.faces[self.index]
    }
}
