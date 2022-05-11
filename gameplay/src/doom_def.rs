use crate::{info::StateNum, MapObject, Player, PspDef};
use std::fmt;

/// Do not know where this is set
pub const TICRATE: i32 = 35;

pub const BFGCELLS: u32 = 40;
pub const MELEERANGE: f32 = 64.0;
pub const MISSILERANGE: f32 = 32.0 * 64.0;
pub const SKULLSPEED: f32 = 20.0;
pub const FLOATSPEED: f32 = 4.0;

/// P_MOBJ
pub static ONFLOORZ: i32 = i32::MIN;
/// P_MOBJ
pub static ONCEILINGZ: i32 = i32::MAX;
pub static MAXHEALTH: i32 = 100;
pub static VIEWHEIGHT: f32 = 41.0;
pub static MAXRADIUS: f32 = 32.0;
pub const USERANGE: f32 = 64.0;

/// DOOM version
pub static DOOM_VERSION: u8 = 109;

/// The maximum number of players, multiplayer/networking.
pub const MAXPLAYERS: usize = 4;
pub const MAX_DEATHMATCH_STARTS: usize = 10;

/// Game mode handling - identify IWAD version to handle IWAD dependend animations etc.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum GameMode {
    /// DOOM 1 shareware, E1, M9
    Shareware,
    /// DOOM 1 registered, E3, M27
    Registered,
    /// DOOM 2 retail, E1 M34
    Commercial,
    /// DOOM 1 retail, E4, M36
    Retail,
    Indetermined, // Well, no IWAD found.
}

// Mission packs - might be useful for TC stuff?
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum GameMission {
    /// Doom (shareware, registered)
    Doom,
    /// Doom II
    Doom2,
    /// TNT mission pack
    PackTnt,
    /// Plutonia mission pack
    PackPlut,
    None,
}

#[derive(Debug, Copy, Clone)]
pub enum GameAction {
    Nothing,
    LoadLevel,
    NewGame,
    LoadGame,
    SaveGame,
    PlayDemo,
    CompletedLevel,
    Victory,
    WorldDone,
    Screenshot,
}

/// Deaf monsters/do not react to sound.
pub static MTF_AMBUSH: i16 = 8;

/// A single flag used to determine if the thing options are multiplayer of singleplayer enabled.
pub const MTF_SINGLE_PLAYER: i16 = 16;

/// Key cards.
#[derive(Copy, Clone)]
pub enum Card {
    Bluecard,
    Yellowcard,
    Redcard,
    Blueskull,
    Yellowskull,
    Redskull,
    NumCards,
}

/// The defined weapons, including a marker indicating user has not changed weapon.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum WeaponType {
    Fist,
    Pistol,
    Shotgun,
    Chaingun,
    Missile,
    Plasma,
    BFG,
    Chainsaw,
    SuperShotgun,
    NumWeapons,
    // No pending weapon change.
    NoChange,
}

impl From<WeaponType> for usize {
    fn from(w: WeaponType) -> Self {
        match w {
            WeaponType::Fist => 0,
            WeaponType::Pistol => 1,
            WeaponType::Shotgun => 2,
            WeaponType::Chaingun => 3,
            WeaponType::Missile => 4,
            WeaponType::Plasma => 5,
            WeaponType::BFG => 6,
            WeaponType::Chainsaw => 7,
            WeaponType::SuperShotgun => 8,
            _ => 0,
        }
    }
}

impl Default for WeaponType {
    fn default() -> Self {
        Self::Pistol
    }
}

impl From<u8> for WeaponType {
    fn from(w: u8) -> Self {
        if w >= WeaponType::NumWeapons as u8 {
            panic!("{} is not a variant of WeaponType", w);
        }
        unsafe { std::mem::transmute(w) }
    }
}

pub const MAX_AMMO: [u32; 4] = [200, 50, 300, 50];
pub const CLIP_AMMO: [u32; 4] = [10, 4, 20, 1];

/// Ammunition types defined.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AmmoType {
    /// Pistol / chaingun ammo.
    Clip,
    /// Shotgun / double barreled shotgun.
    Shell,
    /// Plasma rifle, BFG.
    Cell,
    /// Missile launcher.
    Missile,
    NumAmmo,
    /// Unlimited for chainsaw / fist.
    NoAmmo,
}

impl From<usize> for AmmoType {
    fn from(i: usize) -> Self {
        match i {
            0 => AmmoType::Clip,
            1 => AmmoType::Shell,
            2 => AmmoType::Cell,
            3 => AmmoType::Missile,
            4 => AmmoType::NumAmmo,
            5 => AmmoType::NoAmmo,
            _ => AmmoType::NoAmmo,
        }
    }
}

/// Power up artifacts.
#[derive(Copy, Clone)]
pub enum PowerType {
    Invulnerability,
    Strength,
    Invisibility,
    IronFeet,
    Allmap,
    Infrared,
    NumPowers,
}

/// Power up durations: how many seconds till expiration, assuming TICRATE is 35 ticks/second.
#[derive(Copy, Clone)]
pub enum PowerDuration {
    Invulnerability = (30 * TICRATE) as isize,
    Invisibility = (61 * TICRATE) as isize,
    // TODO: fix back to 60
    Infrared = (120 * TICRATE) as isize,
    IronFeet = (60 * TICRATE) as isize,
}

/// Definition for player sprites (HUD weapon) actions
pub struct WeaponInfo {
    /// Ammto type required
    pub ammo: AmmoType,
    /// The starting state for bringing the weapon up
    pub upstate: StateNum,
    /// The state for putting weapon down
    pub downstate: StateNum,
    /// State for when weapon is *ready to fire*
    pub readystate: StateNum,
    /// State for weapon is firing
    pub atkstate: StateNum,
    /// Muzzle flashes
    pub flashstate: StateNum,
}

pub const WEAPON_INFO: [WeaponInfo; 9] = [
    // fist
    WeaponInfo {
        ammo: AmmoType::NoAmmo,
        upstate: StateNum::PUNCHUP,
        downstate: StateNum::PUNCHDOWN,
        readystate: StateNum::PUNCH,
        atkstate: StateNum::PUNCH1,
        flashstate: StateNum::None,
    },
    // pistol
    WeaponInfo {
        ammo: AmmoType::Clip,
        upstate: StateNum::PISTOLUP,
        downstate: StateNum::PISTOLDOWN,
        readystate: StateNum::PISTOL,
        atkstate: StateNum::PISTOL1,
        flashstate: StateNum::PISTOLFLASH,
    },
    // shotgun
    WeaponInfo {
        ammo: AmmoType::Shell,
        upstate: StateNum::SGUNUP,
        downstate: StateNum::SGUNDOWN,
        readystate: StateNum::SGUN,
        atkstate: StateNum::SGUN1,
        flashstate: StateNum::SGUNFLASH1,
    },
    // chaingun
    WeaponInfo {
        ammo: AmmoType::Clip,
        upstate: StateNum::CHAINUP,
        downstate: StateNum::CHAINDOWN,
        readystate: StateNum::CHAIN,
        atkstate: StateNum::CHAIN1,
        flashstate: StateNum::CHAINFLASH1,
    },
    // missile
    WeaponInfo {
        ammo: AmmoType::Missile,
        upstate: StateNum::MISSILEUP,
        downstate: StateNum::MISSILEDOWN,
        readystate: StateNum::MISSILE,
        atkstate: StateNum::MISSILE1,
        flashstate: StateNum::MISSILEFLASH1,
    },
    // plasma
    WeaponInfo {
        ammo: AmmoType::Cell,
        upstate: StateNum::PLASMAUP,
        downstate: StateNum::PLASMADOWN,
        readystate: StateNum::PLASMA,
        atkstate: StateNum::PLASMA1,
        flashstate: StateNum::PLASMAFLASH1,
    },
    // Big Fucking Gun
    WeaponInfo {
        ammo: AmmoType::Cell,
        upstate: StateNum::BFGUP,
        downstate: StateNum::BFGDOWN,
        readystate: StateNum::BFG,
        atkstate: StateNum::BFG1,
        flashstate: StateNum::BFGFLASH1,
    },
    // chainsaw
    WeaponInfo {
        ammo: AmmoType::NoAmmo,
        upstate: StateNum::SAWUP,
        downstate: StateNum::SAWDOWN,
        readystate: StateNum::SAW,
        atkstate: StateNum::SAW1,
        flashstate: StateNum::None,
    },
    // shotgun
    WeaponInfo {
        ammo: AmmoType::Shell,
        upstate: StateNum::DSGUNUP,
        downstate: StateNum::DSGUNDOWN,
        readystate: StateNum::DSGUN,
        atkstate: StateNum::DSGUN1,
        flashstate: StateNum::DSGUNFLASH1,
    },
];

#[derive(Clone)]
pub enum ActFn {
    /// Pointer to a function that operates on `MapObject`'s. Much of the gamplay uses this (items, monsters etc)
    A(fn(&mut MapObject)),
    /// Pointer to a function that operates on the `Player`, usually also requiring a sprite definition
    P(fn(&mut Player, &mut PspDef)),
    /// For a state with no action
    N,
}

impl fmt::Debug for ActFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActFn::N => f.debug_struct("None").finish(),
            ActFn::A(_) => f.debug_struct("Actor").finish(),
            ActFn::P(_) => f.debug_struct("Player").finish(),
        }
    }
}
