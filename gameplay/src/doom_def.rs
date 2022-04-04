/// Do not know where this is set
pub const TICRATE: i32 = 35;

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
#[allow(non_camel_case_types)]
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
    NUMCARDS,
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

    NUMWEAPONS,

    // No pending weapon change.
    NoChange,
}

pub const MAX_AMMO: [u32; 4] = [200, 50, 300, 50];
pub const CLIP_AMMO: [u32; 4] = [10, 4, 20, 1];

/// Ammunition types defined.
#[derive(Copy, Clone)]
pub enum AmmoType {
    /// Pistol / chaingun ammo.
    Clip,
    /// Shotgun / double barreled shotgun.
    Shell,
    /// Plasma rifle, BFG.
    Cell,
    /// Missile launcher.
    Missile,
    NUMAMMO,
    /// Unlimited for chainsaw / fist.
    NoAmmo,
}

/// Power up artifacts.
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum PowerType {
    Invulnerability,
    Strength,
    Invisibility,
    IronFeet,
    Allmap,
    Infrared,
    NUMPOWERS,
}

/// Power up durations: how many seconds till expiration, assuming TICRATE is 35 ticks/second.
#[derive(Copy, Clone)]
pub enum PowerDuration {
    INVULNTICS = (30 * TICRATE) as isize,
    INVISTICS = (61 * TICRATE) as isize,
    // TODO: fix back to 60
    INFRATICS = (120 * TICRATE) as isize,
    IRONTICS = (60 * TICRATE) as isize,
}
