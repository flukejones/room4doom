use wad::{lumps::SubSector, DPtr, Vertex};

use crate::map_object::MapObject;
use crate::{
    angle::Angle,
    doom_def::{AmmoType, Card, PowerType, WeaponType, MAXPLAYERS},
    info::State,
};

/// Overlay psprites are scaled shapes
/// drawn directly on the view screen,
/// coordinates are given for a 320*200 view screen.
///
/// From P_PSPR
#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum PsprNum {
    ps_weapon,
    ps_flash,
    NUMPSPRITES,
}

/// From P_PSPR
#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct PspDef {
    /// a NULL state means not active
    state: Option<State>,
    tics:  i32,
    sx:    f32,
    sy:    f32,
}

//// Player states.
#[derive(Debug, PartialEq)]
pub enum PlayerState {
    /// Playing or camping.
    PstLive,
    /// Dead on the ground, view follows killer.
    PstDead,
    /// Ready to restart/respawn???
    PstReborn,
}

//// Player internal flags, for cheats and debug.
#[derive(Debug)]
enum Cheat {
    /// No clipping, walk through barriers.
    CfNoclip = 1,
    /// No damage, no health loss.
    CfGodmode = 2,
    /// Not really a cheat, just a debug aid.
    CfNomomentum = 4,
}

// INTERMISSION
// Structure passed e.g. to WI_Start(wb)
pub struct WBPlayerStruct {
    /// whether the player is in game
    pub inn:     bool,
    // Player stats, kills, collected items etc.
    pub skills:  i32,
    pub sitems:  i32,
    pub ssecret: i32,
    pub stime:   i32,
    pub frags:   [i32; 4],
    /// current score on entry, modified on return
    pub score:   i32,
}

pub struct WBStartStruct {
    /// episode # (0-2)
    pub epsd:      i32,
    /// if true, splash the secret level
    pub didsecret: bool,
    /// previous and next levels, origin 0
    pub last:      i32,
    pub next:      i32,
    pub maxkills:  i32,
    pub maxitems:  i32,
    pub maxsecret: i32,
    pub maxfrags:  i32,
    /// the par time
    pub partime:   i32,
    /// index of this player in game
    pub pnum:      i32,
    pub plyr:      [WBPlayerStruct; MAXPLAYERS as usize],
}

/// player_t
#[derive(Debug)]
pub struct Player<'p> {
    // TODO: move these to mapobject
    pub xy:         Vertex,
    pub rotation:   Angle,
    pub sub_sector: DPtr<SubSector>,

    pub mo:          Option<MapObject<'p>>,
    pub playerstate: PlayerState,

    /// Determine POV,
    ///  including viewpoint bobbing during movement.
    /// Focal origin above r.z
    pub viewz:           f32,
    /// Base height above floor for viewz.
    pub viewheight:      f32,
    /// Bob/squat speed.
    pub deltaviewheight: f32,
    /// bounded/scaled total momentum.
    pub bob:             f32,

    /// This is only used between levels,
    /// mo->health is used during levels.
    pub health:      i32,
    pub armorpoints: i32,
    /// Armor type is 0-2.
    pub armortype:   i32,

    /// Power ups. invinc and invis are tic counters.
    pub powers:   [i32; PowerType::NUMPOWERS as usize],
    pub cards:    [bool; Card::NUMCARDS as usize],
    pub backpack: bool,

    /// Frags, kills of other players.
    frags:       [i32; MAXPLAYERS as usize],
    readyweapon: WeaponType,

    /// Is wp_nochange if not changing.
    pendingweapon: WeaponType,

    weaponowned: [i32; WeaponType::NUMWEAPONS as usize],
    ammo:        [i32; AmmoType::NUMAMMO as usize],
    maxammo:     [i32; AmmoType::NUMAMMO as usize],

    /// True if button down last tic.
    attackdown: bool,
    usedown:    bool,

    /// Bit flags, for cheats and debug.
    /// See cheat_t, above.
    cheats: i32,

    /// Refired shots are less accurate.
    pub refire: i32,

    /// For intermission stats.
    killcount:   i32,
    itemcount:   i32,
    secretcount: i32,

    /// Hint messages.
    pub message: Option<&'p str>,

    /// For screen flashing (red or bright).
    pub damagecount: i32,
    pub bonuscount:  i32,

    // Who did damage (NULL for floors/ceilings).
    //mobj_t*		attacker;
    /// So gun flashes light up areas.
    pub extralight: i32,

    /// Current PLAYPAL, ???
    ///  can be set to REDCOLORMAP for pain, etc.
    pub fixedcolormap: i32,

    /// Player skin colorshift,
    ///  0-3 for which color to draw player.
    colormap: i32,

    /// Overlay view sprites (gun, etc).
    psprites: [PspDef; PsprNum::NUMPSPRITES as usize],

    /// True if secret level has been done.
    didsecret: bool,
}

impl<'p> Player<'p> {
    pub const fn new(
        xy: Vertex,
        z: f32,
        rotation: Angle,
        sub_sector: DPtr<SubSector>,
        mo: Option<MapObject<'p>>, // TODO: should be a pointer
    ) -> Player<'p> {
        Player {
            xy,
            viewz: z,
            rotation,
            sub_sector,
            mo,

            viewheight: 41.0,
            deltaviewheight: 41.0,
            bob: 3.0,
            health: 100,
            armorpoints: 0,
            armortype: 0,
            ammo: [0; AmmoType::NUMAMMO as usize],
            maxammo: [0; AmmoType::NUMAMMO as usize],
            powers: [0; PowerType::NUMPOWERS as usize],
            cards: [false; Card::NUMCARDS as usize],
            backpack: false,
            attackdown: false,
            usedown: false,
            cheats: 0,
            refire: 0,

            killcount: 0,
            itemcount: 0,
            secretcount: 0,

            message: None,
            damagecount: 0,
            bonuscount: 0,

            colormap: 0,
            didsecret: false,
            extralight: 0,
            fixedcolormap: 0,

            frags: [0; 4],
            readyweapon: WeaponType::wp_pistol,
            pendingweapon: WeaponType::NUMWEAPONS,
            weaponowned: [0; WeaponType::NUMWEAPONS as usize],

            playerstate: PlayerState::PstReborn,

            psprites: [
                PspDef {
                    state: None,
                    tics:  1,
                    sx:    0.0,
                    sy:    0.0,
                },
                PspDef {
                    state: None,
                    tics:  1,
                    sx:    0.0,
                    sy:    0.0,
                },
            ],
        }
    }
    // TODO: needs p_pspr.c, p_inter.c
}
