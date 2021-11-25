/// Do not know where this is set
pub const TICRATE: i32 = 35;

/// DOOM version
pub static DOOM_VERSION: u8 = 109;

/// Version code for cph's longtics hack ("v1.91")
pub static DOOM_191_VERSION: u8 = 111;

/// The maximum number of players, multiplayer/networking.
pub const MAXPLAYERS: usize = 4;
pub const MAX_DEATHMATCH_STARTS: usize = 10;

pub const BACKUPTICS: usize = 12;

pub const ML_DONTPEGBOTTOM: u32 = 16;
pub const ML_MAPPED: u32 = 256;

// Game mode handling - identify IWAD version
//  to handle IWAD dependend animations etc.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum GameMode {
    Shareware,
    // DOOM 1 shareware, E1, M9
    Registered,
    // DOOM 1 registered, E3, M27
    Commercial,
    // DOOM 2 retail, E1 M34
    // DOOM 2 german edition not handled
    Retail,
    // DOOM 1 retail, E4, M36
    Indetermined, // Well, no IWAD found.
}

// Mission packs - might be useful for TC stuff?
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum GameMission {
    Doom,
    // DOOM 1
    Doom2,
    // DOOM 2
    PackTnt,
    // TNT mission pack
    PackPlut,
    // Plutonia pack
    None,
}

/// The current state of the game: whether we are
/// playing, gazing at the intermission screen,
/// the game final animation, or a demo.
#[derive(Debug, Copy, Clone, PartialEq)]
#[allow(non_camel_case_types)]
pub enum GameState {
    FORCE_WIPE = -1,
    GS_LEVEL,
    GS_INTERMISSION,
    GS_FINALE,
    GS_DEMOSCREEN,
}

#[derive(Debug, Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum GameAction {
    ga_nothing,
    ga_loadlevel,
    ga_newgame,
    ga_loadgame,
    ga_savegame,
    ga_playdemo,
    ga_completed,
    ga_victory,
    ga_worlddone,
    ga_screenshot,
}

// Difficulty/skill settings/filters.

/// Skill flags.
pub static MTF_EASY: u8 = 1;
pub static MTF_NORMAL: u8 = 2;
pub static MTF_HARD: u8 = 4;

/// Deaf monsters/do not react to sound.
pub static MTF_AMBUSH: i16 = 8;

/// Key cards.
#[derive(Debug, Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum Card {
    it_bluecard,
    it_yellowcard,
    it_redcard,
    it_blueskull,
    it_yellowskull,
    it_redskull,

    NUMCARDS,
}

/// The defined weapons, including a marker indicating user has not changed weapon.
#[derive(Debug, Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum WeaponType {
    wp_fist,
    wp_pistol,
    wp_shotgun,
    wp_chaingun,
    wp_missile,
    wp_plasma,
    wp_bfg,
    wp_chainsaw,
    wp_supershotgun,

    NUMWEAPONS,

    // No pending weapon change.
    wp_nochange,
}

pub const MAX_AMMO: [u32; 4] = [200, 50, 300, 50];
pub const CLIP_AMMO: [u32; 4] = [10, 4, 20, 1];

/// Ammunition types defined.
#[derive(Debug, Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum AmmoType {
    /// Pistol / chaingun ammo.
    am_clip,
    /// Shotgun / double barreled shotgun.
    am_shell,
    /// Plasma rifle, BFG.
    am_cell,
    /// Missile launcher.
    am_misl,
    NUMAMMO,
    /// Unlimited for chainsaw / fist.
    am_noammo,
}

/// Power up artifacts.
#[derive(Debug, Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum PowerType {
    pw_invulnerability,
    pw_strength,
    pw_invisibility,
    pw_ironfeet,
    pw_allmap,
    pw_infrared,
    NUMPOWERS,
}

/// Power up durations,
///  how many seconds till expiration,
///  assuming TICRATE is 35 ticks/second.
#[derive(Debug, Copy, Clone)]
pub enum PowerDuration {
    INVULNTICS = (30 * TICRATE) as isize,
    INVISTICS = (61 * TICRATE) as isize,
    // TODO: fix back to 60
    INFRATICS = (120 * TICRATE) as isize,
    IRONTICS = (60 * TICRATE) as isize,
}
