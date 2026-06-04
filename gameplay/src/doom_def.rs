use crate::info::StateNum;
use crate::player_sprite::*;
use crate::thing::enemy::*;
use crate::{MapObject, Player, PspDef};

/// Do not know where this is set
pub const TICRATE: i32 = 35;

pub const BFGCELLS: u32 = 40;
pub const MELEERANGE: i32 = 64;
pub const MISSILERANGE: i32 = 32 * 64;
pub const FLOATSPEED: i32 = 4;

/// P_MOBJ
pub static ONFLOORZ: i32 = i32::MIN;
/// P_MOBJ
pub static ONCEILINGZ: i32 = i32::MAX;
pub static MAXHEALTH: i32 = 100;
pub static VIEWHEIGHT: i32 = 41;
pub static MAXRADIUS: i32 = 32;
pub const USERANGE: i32 = 64;

/// DOOM version
pub static DOOM_VERSION: u8 = 109;

/// The maximum number of players, multiplayer/networking.
pub const MAXPLAYERS: usize = 4;
pub const MAX_DEATHMATCH_STARTS: usize = 10;
pub const MAX_RESPAWNS: usize = 128;

#[derive(Debug, Copy, Clone)]
pub enum GameAction {
    /// No action required
    None,
    /// Load a game level (requires a level number and episode to be set
    /// beforehand)
    LoadLevel,
    /// Resets the entire game state and starts a new level/episode
    NewGame,
    LoadGame,
    SaveGame,
    PlayDemo,
    /// The player finished the level
    CompletedLevel,
    /// Level finished and intermission screen completed
    WorldDone,
    /// The player finished the game or episode
    Victory,
    Screenshot,
}

/// Deaf monsters/do not react to sound.
pub static MTF_AMBUSH: i16 = 8;

/// A single flag used to determine if the thing options are multiplayer of
/// singleplayer enabled.
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
    /// Used as a marker to count total available ammo types
    NumAmmo,
    /// Unlimited for chainsaw / fist.
    NoAmmo,
}

impl From<usize> for AmmoType {
    fn from(i: usize) -> Self {
        match i {
            0 => Self::Clip,
            1 => Self::Shell,
            2 => Self::Cell,
            3 => Self::Missile,
            4 => Self::NumAmmo,
            _ => Self::NoAmmo,
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
    /// Used as a marker to count total available power types
    NumPowers,
}

/// Power up durations: how many seconds till expiration, assuming TICRATE is 35
/// ticks/second.
#[derive(Copy, Clone)]
pub enum PowerDuration {
    Invulnerability = (30 * TICRATE) as isize,
    Invisibility = (61 * TICRATE) as isize,
    Infrared = (120 * TICRATE) as isize,
    IronFeet = (60 * TICRATE) as isize,
}

/// Definition for player sprites (HUD weapon) actions
pub struct WeaponInfo {
    /// Amto type required
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

use crate::info::ActionId;

impl ActionId {
    /// Resolve to an actor action function, or None if this is a player action
    /// or None.
    pub fn resolve_actor(&self) -> Option<fn(&mut MapObject)> {
        match self {
            Self::ABfgspray => Some(a_bfgspray),
            Self::AExplode => Some(a_explode),
            Self::APain => Some(a_pain),
            Self::APlayerscream => Some(a_playerscream),
            Self::AFall => Some(a_fall),
            Self::AXscream => Some(a_xscream),
            Self::ALook => Some(a_look),
            Self::AChase => Some(a_chase),
            Self::AFacetarget => Some(a_facetarget),
            Self::APosattack => Some(a_posattack),
            Self::AScream => Some(a_scream),
            Self::ASposattack => Some(a_sposattack),
            Self::AVilechase => Some(a_vilechase),
            Self::AVilestart => Some(a_vilestart),
            Self::AViletarget => Some(a_viletarget),
            Self::AVileattack => Some(a_vileattack),
            Self::AStartfire => Some(a_startfire),
            Self::AFire => Some(a_fire),
            Self::AFirecrackle => Some(a_firecrackle),
            Self::ATracer => Some(a_tracer),
            Self::ASkelwhoosh => Some(a_skelwhoosh),
            Self::ASkelfist => Some(a_skelfist),
            Self::ASkelmissile => Some(a_skelmissile),
            Self::AFatraise => Some(a_fatraise),
            Self::AFatattack1 => Some(a_fatattack1),
            Self::AFatattack2 => Some(a_fatattack2),
            Self::AFatattack3 => Some(a_fatattack3),
            Self::ABossdeath => Some(a_bossdeath),
            Self::ACposattack => Some(a_cposattack),
            Self::ACposrefire => Some(a_cposrefire),
            Self::ATroopattack => Some(a_troopattack),
            Self::ASargattack => Some(a_sargattack),
            Self::AHeadattack => Some(a_headattack),
            Self::ABruisattack => Some(a_bruisattack),
            Self::ASkullattack => Some(a_skullattack),
            Self::AMetal => Some(a_metal),
            Self::ASpidrefire => Some(a_spidrefire),
            Self::ABabymetal => Some(a_babymetal),
            Self::ABspiattack => Some(a_bspiattack),
            Self::AHoof => Some(a_hoof),
            Self::ACyberattack => Some(a_cyberattack),
            Self::APainattack => Some(a_painattack),
            Self::APaindie => Some(a_paindie),
            Self::AKeendie => Some(a_keendie),
            Self::ABrainpain => Some(a_brainpain),
            Self::ABrainscream => Some(a_brainscream),
            Self::ABraindie => Some(a_braindie),
            Self::ABrainawake => Some(a_brainawake),
            Self::ABrainspit => Some(a_brainspit),
            Self::ASpawnsound => Some(a_spawnsound),
            Self::ASpawnfly => Some(a_spawnfly),
            Self::ABrainexplode => Some(a_brainexplode),
            _ => None,
        }
    }

    /// Resolve to a player action function, or None if this is an actor action
    /// or None.
    pub fn resolve_player(&self) -> Option<fn(&mut Player, &mut PspDef)> {
        match self {
            Self::PLight0 => Some(a_light0),
            Self::PWeaponready => Some(a_weaponready),
            Self::PLower => Some(a_lower),
            Self::PRaise => Some(a_raise),
            Self::PPunch => Some(a_punch),
            Self::PRefire => Some(a_refire),
            Self::PFirepistol => Some(a_firepistol),
            Self::PLight1 => Some(a_light1),
            Self::PFireshotgun => Some(a_fireshotgun),
            Self::PLight2 => Some(a_light2),
            Self::PFireshotgun2 => Some(a_fireshotgun2),
            Self::PCheckreload => Some(a_checkreload),
            Self::POpenshotgun2 => Some(a_openshotgun2),
            Self::PLoadshotgun2 => Some(a_loadshotgun2),
            Self::PCloseshotgun2 => Some(a_closeshotgun2),
            Self::PFirecgun => Some(a_firecgun),
            Self::PGunflash => Some(a_gunflash),
            Self::PFiremissile => Some(a_firemissile),
            Self::PSaw => Some(a_saw),
            Self::PFireplasma => Some(a_fireplasma),
            Self::PBfgsound => Some(a_bfgsound),
            Self::PFirebfg => Some(a_firebfg),
            _ => None,
        }
    }
}
