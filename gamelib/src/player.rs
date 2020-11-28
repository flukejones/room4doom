use std::f32::consts::{FRAC_PI_2, PI};

use glam::Vec2;
use wad::{lumps::SubSector, DPtr, Vertex};

/// 16 pixels of bob
const MAXBOB: f32 = 1.0; // 0x100000;

use crate::{
    angle::Angle,
    doom_def::{AmmoType, Card, PowerType, WeaponType, MAXPLAYERS},
    info::StateNum,
    p_local::VIEWHEIGHT,
};
use crate::{
    d_thinker::{Think, Thinker},
    info::SpriteNum,
    p_local::bam_to_radian,
    p_local::fixed_to_float,
    p_map_object::MapObject,
    tic_cmd::TicCmd,
};
use crate::{level::Level, p_player_sprite::PspDef};

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
    CfNoclip     = 1,
    /// No damage, no health loss.
    CfGodmode    = 2,
    /// Not really a cheat, just a debug aid.
    CfNomomentum = 4,
}

/// INTERMISSION
/// Structure passed e.g. to WI_Start(wb)
#[derive(Debug, Default)]
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

/// parms for world map / intermission
#[derive(Debug, Default)]
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

const NUM_POWERS: usize = PowerType::NUMPOWERS as usize;
const NUM_CARDS: usize = Card::NUMCARDS as usize;
const NUM_WEAPONS: usize = WeaponType::NUMWEAPONS as usize;
const NUM_AMMO: usize = AmmoType::NUMAMMO as usize;
const NUM_SPRITES: usize = PsprNum::NUMPSPRITES as usize;

/// player_t
#[derive(Debug)]
pub struct Player {
    pub mo:           Option<Thinker<MapObject>>,
    pub player_state: PlayerState,
    pub cmd:          TicCmd,

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
    pub powers:   [i32; NUM_POWERS],
    pub cards:    [bool; NUM_CARDS],
    pub backpack: bool,

    /// Frags, kills of other players.
    pub frags:   [i32; MAXPLAYERS as usize],
    readyweapon: WeaponType,

    /// Is wp_nochange if not changing.
    pendingweapon: WeaponType,

    weaponowned: [i32; NUM_WEAPONS],
    ammo:        [i32; NUM_AMMO],
    maxammo:     [i32; NUM_AMMO],

    /// True if button down last tic.
    attackdown: bool,
    usedown:    bool,

    /// Bit flags, for cheats and debug.
    /// See cheat_t, above.
    cheats: i32,

    /// Refired shots are less accurate.
    pub refire: i32,

    /// For intermission stats.
    pub killcount:   i32,
    pub itemcount:   i32,
    pub secretcount: i32,

    /// Hint messages.
    pub message: Option<String>,

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
    psprites: [PspDef; NUM_SPRITES],

    /// True if secret level has been done.
    didsecret: bool,
}

impl Default for Player {
    fn default() -> Self {
        Player::new(Vertex::new(0.0, 0.0), 0.0, Angle::new(0.0), None, None)
    }
}

impl Player {
    pub const fn new(
        xy: Vertex,
        z: f32,
        rotation: Angle,
        sub_sector: Option<DPtr<SubSector>>,
        mo: Option<Thinker<MapObject>>, // TODO: should be a pointer
    ) -> Player {
        Player {
            viewz: z,
            mo,

            viewheight: 41.0,
            deltaviewheight: 1.0,
            bob: 3.0,
            health: 100,
            armorpoints: 0,
            armortype: 0,
            ammo: [0; NUM_AMMO],
            maxammo: [0; NUM_AMMO],
            powers: [0; NUM_POWERS],
            cards: [false; NUM_CARDS],
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
            weaponowned: [0; NUM_WEAPONS],

            player_state: PlayerState::PstReborn,
            cmd: TicCmd::new(),

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

    fn thrust(&mut self, angle: Angle, mv: i32) {
        // mv is in a fixed float format, we need to convert it
        // TODO: make some of this constant later
        let mv = fixed_to_float(mv);
        let x = mv as f32 * angle.cos();
        let y = mv as f32 * angle.sin();
        let mxy = Vec2::new(x, y);

        if let Some(ref mut thinker) = self.mo {
            thinker.obj.momxy += mxy;
        }
    }

    /// P_CalcHeight
    /// Calculate the walking / running height adjustment
    fn calculate_height(&mut self, level_time: i32) {
        // Regular movement bobbing
        // (needs to be calculated for gun swing
        // even if not on ground)
        // OPTIMIZE: tablify angle
        // Note: a LUT allows for effects
        //  like a ramp with low health.
        if let Some(ref mut mo) = self.mo {
            let x = mo.obj.momxy.x();
            let y = mo.obj.momxy.y();
            self.bob = x * x + y * y;

            //self.bob >>= 2;

            if self.bob > MAXBOB {
                self.bob = MAXBOB;
            }

            // TODO: if ((player->cheats & CF_NOMOMENTUM) || !onground)
            // if self.viewz > mo.obj.ceilingz - 4.0 {
            //     self.viewz = mo.obj.ceilingz - 4.0;
            // }

            // BOB IS WRONG!
            let angle = ((8192 / 20 * level_time) & 8191) as f32; // / 180.0 * PI;
            let bob = self.bob / 2.0 * angle.sin();

            // move viewheight
            if self.player_state == PlayerState::PstLive {
                self.viewheight += self.deltaviewheight;

                if self.viewheight > VIEWHEIGHT {
                    self.viewheight = VIEWHEIGHT;
                    self.deltaviewheight = 0.0;
                }

                if self.viewheight < VIEWHEIGHT / 2.0 {
                    self.viewheight = VIEWHEIGHT / 2.0;
                    if self.deltaviewheight <= 0.0 {
                        self.deltaviewheight = 1.0;
                    }
                }

                if self.deltaviewheight > 0.0 {
                    self.deltaviewheight = 0.25;
                    if self.deltaviewheight <= 0.0 {
                        self.deltaviewheight = 1.0;
                    }
                }
            }

            self.viewz = mo.obj.z + self.viewheight + bob;

            if self.viewz > mo.obj.ceilingz - 4.0 {
                self.viewz = mo.obj.ceilingz - 4.0;
            }
        }
    }

    fn move_player(&mut self) {
        // TODO: Fix adjustments after fixing the tic timestep
        if self.cmd.angleturn != 0 {
            let a = bam_to_radian((self.cmd.angleturn as u32) << 16);
            self.mo.as_mut().unwrap().obj.angle += a;
        }

        if self.cmd.forwardmove != 0 {
            let angle = self.mo.as_mut().unwrap().obj.angle;
            self.thrust(angle, self.cmd.forwardmove as i32 * 2048);
        }

        if self.cmd.sidemove != 0 {
            let angle = self.mo.as_mut().unwrap().obj.angle;
            self.thrust(angle - FRAC_PI_2, self.cmd.sidemove as i32 * 2048);
        }

        if self.cmd.forwardmove != 0 || self.cmd.sidemove != 0 {
            if let Some(ref thinker) = self.mo {
                if thinker.obj.state.sprite as i32 == SpriteNum::SPR_PLAY as i32
                {
                    //P_SetMobjState (player->mo, S_PLAY_RUN1);
                }
            }
        }
    }
}

impl Think for Player {
    fn think(&mut self, level: &mut Level) -> bool {
        self.move_player();
        self.calculate_height(level.level_time);

        if let Some(ref mut mo) = self.mo {
            mo.think(level); // Player own the thinker, so make it think here
        }
        false
    }
}
