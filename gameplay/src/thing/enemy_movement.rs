use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use log::error;

use crate::doom_def::FLOATSPEED;
use crate::env::switch::p_use_special_line;
use crate::{p_random, Angle, MapObjFlag, MapObject};

use super::movement::SubSectorMinMax;

#[repr(usize)]
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub(crate) enum MoveDir {
    East,
    NorthEast,
    North,
    NorthWest,
    West,
    SouthWest,
    South,
    SouthEast,
    None,
    NumDirs,
}

impl From<usize> for MoveDir {
    fn from(w: usize) -> Self {
        if w >= MoveDir::NumDirs as usize {
            panic!("{} is not a variant of DirType", w);
        }
        unsafe { std::mem::transmute(w) }
    }
}

impl From<MoveDir> for Angle {
    fn from(d: MoveDir) -> Angle {
        match d {
            MoveDir::East => Angle::default(),
            MoveDir::NorthEast => Angle::new(FRAC_PI_4),
            MoveDir::North => Angle::new(FRAC_PI_2),
            MoveDir::NorthWest => Angle::new(FRAC_PI_2 + FRAC_PI_4),
            MoveDir::West => Angle::new(PI),
            MoveDir::SouthWest => Angle::new(PI + FRAC_PI_4),
            MoveDir::South => Angle::new(PI + FRAC_PI_2),
            MoveDir::SouthEast => Angle::new(PI + FRAC_PI_2 + FRAC_PI_4),
            _ => Angle::default(),
        }
    }
}

const DIR_OPPOSITE: [MoveDir; 9] = [
    MoveDir::West,
    MoveDir::SouthWest,
    MoveDir::South,
    MoveDir::SouthEast,
    MoveDir::East,
    MoveDir::NorthEast,
    MoveDir::North,
    MoveDir::NorthWest,
    MoveDir::None,
];

const DIR_DIAGONALS: [MoveDir; 4] = [
    MoveDir::NorthWest,
    MoveDir::NorthEast,
    MoveDir::SouthWest,
    MoveDir::SouthEast,
];

const DIR_XSPEED: [f32; 8] = [1.0, 0.47, 0.0, -0.47, -1.0, -0.47, 0.0, 0.47];
const DIR_YSPEED: [f32; 8] = [0.0, 0.47, 1.0, 0.47, 0.0, -0.47, -1.0, -0.47];

impl MapObject {
    /// Try to move in current direction. If blocked by a wall or other actor it
    /// returns false, otherwise tries to open a door if the block is one, and
    /// continue.
    #[inline]
    fn try_walk(&mut self) -> bool {
        if !self.do_enemy_move() {
            return false;
        }
        self.movecount = p_random() & 15;
        true
    }

    pub(crate) fn do_enemy_move(&mut self) -> bool {
        if self.movedir == MoveDir::None {
            return false;
        }

        let mut try_move = self.xyz;
        try_move.x += self.info.speed * DIR_XSPEED[self.movedir as usize];
        try_move.y += self.info.speed * DIR_YSPEED[self.movedir as usize];

        let mut specs = SubSectorMinMax::default();
        if !self.p_try_move(try_move, &mut specs) {
            // open any specials
            // TODO: if (actor->flags & MF_FLOAT && floatok)
            if self.flags & MapObjFlag::Float as u32 != 0 && specs.floatok {
                // must adjust height
                if self.xyz.z < specs.min_floor_z {
                    self.xyz.z += FLOATSPEED;
                } else {
                    self.xyz.z -= FLOATSPEED;
                }
                self.flags |= MapObjFlag::Infloat as u32;
                return true;
            }

            if specs.spec_hits.is_empty() {
                return false;
            }

            self.movedir = MoveDir::None;
            let mut good = false;
            for ld in &specs.spec_hits {
                if p_use_special_line(0, ld.clone(), self) || ld.special == 0 {
                    good = true;
                }
            }
            return good;
        } else {
            self.flags &= !(MapObjFlag::Infloat as u32);
        }

        if self.flags & MapObjFlag::Float as u32 == 0 {
            self.xyz.z = self.floorz;
        }

        true
    }

    pub(crate) fn new_chase_dir(&mut self) {
        if self.target.is_none() {
            error!("new_chase_dir called with no target");
            return;
        }

        let old_dir = self.movedir;
        let mut dirs = [MoveDir::None, MoveDir::None, MoveDir::None];
        let turnaround = DIR_OPPOSITE[old_dir as usize];

        let target = unsafe { (**self.target.as_mut().unwrap()).mobj() };

        // if !self.target_within_min_dist(target) {
        //     return;
        // }

        let dx = target.xyz.x - self.xyz.x;
        let dy = target.xyz.y - self.xyz.y;
        // Select a cardinal angle based on delta
        if dx > 10.0 {
            dirs[1] = MoveDir::East;
        } else if dx < -10.0 {
            dirs[1] = MoveDir::West;
        } else {
            dirs[1] = MoveDir::None;
        }

        if dy < -10.0 {
            dirs[2] = MoveDir::South;
        } else if dy > 10.0 {
            dirs[2] = MoveDir::North;
        } else {
            dirs[2] = MoveDir::None;
        }

        // try direct route
        if dirs[1] != MoveDir::None && dirs[2] != MoveDir::None {
            self.movedir =
                DIR_DIAGONALS[(((dy < 0.0) as u32 as usize) << 1) + (dx > 0.0) as u32 as usize];
            if self.movedir != turnaround && self.try_walk() {
                return;
            }
        }

        // try other directions
        if p_random() > 200 || dy.abs() > dx.abs() {
            dirs.swap(1, 2);
        }
        if dirs[1] == turnaround {
            dirs[1] = MoveDir::None;
        }
        if dirs[2] == turnaround {
            dirs[2] = MoveDir::None;
        }

        if dirs[1] != MoveDir::None {
            self.movedir = dirs[1];
            if self.try_walk() {
                // either moved forward or attacked
                return;
            }
        }

        if dirs[2] != MoveDir::None {
            self.movedir = dirs[2];
            if self.try_walk() {
                // either moved forward or attacked
                return;
            }
        }

        // there is no direct path to the player, so pick another direction.
        if old_dir != MoveDir::None {
            self.movedir = old_dir;
            if self.try_walk() {
                return;
            }
        }

        // randomly determine direction of search
        if p_random() & 1 != 0 {
            for t in MoveDir::East as u32 as usize..=MoveDir::SouthEast as u32 as usize {
                let tdir = MoveDir::from(t);
                if tdir != turnaround {
                    self.movedir = tdir;
                    if self.try_walk() {
                        return;
                    }
                }
            }
        } else {
            for t in (MoveDir::East as u32 as usize..=MoveDir::SouthEast as u32 as usize).rev() {
                let tdir = MoveDir::from(t);
                if tdir != turnaround {
                    self.movedir = tdir;
                    if self.try_walk() {
                        return;
                    }
                }
            }
        }

        if turnaround != MoveDir::None {
            self.movedir = turnaround;
            if self.try_walk() {
                return;
            }
        }

        // Can't move
        self.movedir = MoveDir::None;
    }
}
