use crate::info::StateNum;
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
    /// Used as a marker to count total available power types
    NumPowers,
}

/// Power up durations: how many seconds till expiration, assuming TICRATE is 35
/// ticks/second.
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
        use crate::player_sprite::a_bfgspray;
        use crate::thing::enemy::*;
        match self {
            ActionId::ABfgspray => Some(a_bfgspray),
            ActionId::AExplode => Some(a_explode),
            ActionId::APain => Some(a_pain),
            ActionId::APlayerscream => Some(a_playerscream),
            ActionId::AFall => Some(a_fall),
            ActionId::AXscream => Some(a_xscream),
            ActionId::ALook => Some(a_look),
            ActionId::AChase => Some(a_chase),
            ActionId::AFacetarget => Some(a_facetarget),
            ActionId::APosattack => Some(a_posattack),
            ActionId::AScream => Some(a_scream),
            ActionId::ASposattack => Some(a_sposattack),
            ActionId::AVilechase => Some(a_vilechase),
            ActionId::AVilestart => Some(a_vilestart),
            ActionId::AViletarget => Some(a_viletarget),
            ActionId::AVileattack => Some(a_vileattack),
            ActionId::AStartfire => Some(a_startfire),
            ActionId::AFire => Some(a_fire),
            ActionId::AFirecrackle => Some(a_firecrackle),
            ActionId::ATracer => Some(a_tracer),
            ActionId::ASkelwhoosh => Some(a_skelwhoosh),
            ActionId::ASkelfist => Some(a_skelfist),
            ActionId::ASkelmissile => Some(a_skelmissile),
            ActionId::AFatraise => Some(a_fatraise),
            ActionId::AFatattack1 => Some(a_fatattack1),
            ActionId::AFatattack2 => Some(a_fatattack2),
            ActionId::AFatattack3 => Some(a_fatattack3),
            ActionId::ABossdeath => Some(a_bossdeath),
            ActionId::ACposattack => Some(a_cposattack),
            ActionId::ACposrefire => Some(a_cposrefire),
            ActionId::ATroopattack => Some(a_troopattack),
            ActionId::ASargattack => Some(a_sargattack),
            ActionId::AHeadattack => Some(a_headattack),
            ActionId::ABruisattack => Some(a_bruisattack),
            ActionId::ASkullattack => Some(a_skullattack),
            ActionId::AMetal => Some(a_metal),
            ActionId::ASpidrefire => Some(a_spidrefire),
            ActionId::ABabymetal => Some(a_babymetal),
            ActionId::ABspiattack => Some(a_bspiattack),
            ActionId::AHoof => Some(a_hoof),
            ActionId::ACyberattack => Some(a_cyberattack),
            ActionId::APainattack => Some(a_painattack),
            ActionId::APaindie => Some(a_paindie),
            ActionId::AKeendie => Some(a_keendie),
            ActionId::ABrainpain => Some(a_brainpain),
            ActionId::ABrainscream => Some(a_brainscream),
            ActionId::ABraindie => Some(a_braindie),
            ActionId::ABrainawake => Some(a_brainawake),
            ActionId::ABrainspit => Some(a_brainspit),
            ActionId::ASpawnsound => Some(a_spawnsound),
            ActionId::ASpawnfly => Some(a_spawnfly),
            ActionId::ABrainexplode => Some(a_brainexplode),
            _ => None,
        }
    }

    /// Resolve to a player action function, or None if this is an actor action
    /// or None.
    pub fn resolve_player(&self) -> Option<fn(&mut Player, &mut PspDef)> {
        use crate::player_sprite::*;
        match self {
            ActionId::PLight0 => Some(a_light0),
            ActionId::PWeaponready => Some(a_weaponready),
            ActionId::PLower => Some(a_lower),
            ActionId::PRaise => Some(a_raise),
            ActionId::PPunch => Some(a_punch),
            ActionId::PRefire => Some(a_refire),
            ActionId::PFirepistol => Some(a_firepistol),
            ActionId::PLight1 => Some(a_light1),
            ActionId::PFireshotgun => Some(a_fireshotgun),
            ActionId::PLight2 => Some(a_light2),
            ActionId::PFireshotgun2 => Some(a_fireshotgun2),
            ActionId::PCheckreload => Some(a_checkreload),
            ActionId::POpenshotgun2 => Some(a_openshotgun2),
            ActionId::PLoadshotgun2 => Some(a_loadshotgun2),
            ActionId::PCloseshotgun2 => Some(a_closeshotgun2),
            ActionId::PFirecgun => Some(a_firecgun),
            ActionId::PGunflash => Some(a_gunflash),
            ActionId::PFiremissile => Some(a_firemissile),
            ActionId::PSaw => Some(a_saw),
            ActionId::PFireplasma => Some(a_fireplasma),
            ActionId::PBfgsound => Some(a_bfgsound),
            ActionId::PFirebfg => Some(a_firebfg),
            _ => None,
        }
    }
}
