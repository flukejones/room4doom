/// Do not know where this is set
pub const TICRATE: i32 = 35;

/// DOOM version
pub static DOOM_VERSION: u8 = 109;

/// Version code for cph's longtics hack ("v1.91")
pub static DOOM_191_VERSION: u8 = 111;

/// The maximum number of players, multiplayer/networking.
pub const MAXPLAYERS: u8 = 4;

/// The current state of the game: whether we are
/// playing, gazing at the intermission screen,
/// the game final animation, or a demo.
#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum GameState {
    GS_LEVEL,
    GS_INTERMISSION,
    GS_FINALE,
    GS_DEMOSCREEN,
}

#[derive(Debug)]
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
pub static MTF_AMBUSH: u8 = 8;

/// Key cards.
#[derive(Debug)]
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
#[derive(Debug)]
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

/// Ammunition types defined.
#[derive(Debug)]
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
#[derive(Debug)]
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

// Power up durations,
//  how many seconds till expiration,
//  assuming TICRATE is 35 ticks/second.
//
pub enum PowerDuration {
    INVULNTICS = (30 * TICRATE) as isize,
    INVISTICS = (61 * TICRATE) as isize, // TODO: fix back to 60
    INFRATICS = (120 * TICRATE) as isize,
    IRONTICS = (60 * TICRATE) as isize,
}
