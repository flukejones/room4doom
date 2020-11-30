use std::f32::consts::FRAC_PI_2;

use glam::Vec2;

/// 16 pixels of bob
const MAXBOB: f32 = 16.0; // 0x100000;

use crate::{angle::Angle, doom_def::{AmmoType, Card, PowerType, WeaponType, MAXPLAYERS}, p_local::MAXHEALTH, doom_def::MAX_AMMO, p_local::VIEWHEIGHT};
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
    pub mobj:         Option<Thinker<MapObject>>,
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
    pub onground:        bool,

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
    pub readyweapon: WeaponType,

    /// Is wp_nochange if not changing.
    pub pendingweapon: WeaponType,

    pub weaponowned: [bool; NUM_WEAPONS],
    pub ammo:        [u32; NUM_AMMO],
    maxammo:     [u32; NUM_AMMO],

    /// True if button down last tic.
    pub attackdown: bool,
    pub usedown:    bool,

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
    fn default() -> Self { Player::new(None) }
}

impl Player {
    pub const fn new(
        mobj: Option<Thinker<MapObject>>, // TODO: should be a pointer
    ) -> Player {
        Player {
            viewz: 0.0,
            mobj,

            viewheight: 41.0,
            deltaviewheight: 1.0,
            bob: 1.0,
            onground: true,
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
            weaponowned: [false; NUM_WEAPONS],

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

    pub fn player_reborn(&mut self) {
        let kill_count = self.killcount;
        let item_count = self.itemcount;
        let secret_count = self.secretcount;

        *self = Player::default();
        self.killcount = kill_count;
        self.itemcount = item_count;
        self.secretcount = secret_count;

        self.usedown = false;
        self.attackdown = false;
        self.player_state = PlayerState::PstLive;
        self.health = MAXHEALTH;
        self.readyweapon = WeaponType::wp_pistol;
        self.pendingweapon = WeaponType::wp_pistol;
        self.weaponowned[WeaponType::wp_fist as usize] = true;
        self.weaponowned[WeaponType::wp_pistol as usize] = true;
        self.ammo[AmmoType::am_clip as usize] = 50;

        for i in 0..self.maxammo.len() {
            self.maxammo[i] = MAX_AMMO[i];
        }
    }

    fn thrust(&mut self, angle: Angle, mv: i32) {
        // mv is in a fixed float format, we need to convert it
        // TODO: make some of this constant later
        let mv = fixed_to_float(mv);
        let x = mv as f32 * angle.cos();
        let y = mv as f32 * angle.sin();
        let mxy = Vec2::new(x, y);

        if let Some(ref mut thinker) = self.mobj {
            thinker.obj.momxy += mxy;
        }
    }

    /// P_CalcHeight
    /// Calculate the walking / running height adjustment
    fn calculate_height(&mut self, level_time: u32) {
        // Regular movement bobbing
        // (needs to be calculated for gun swing
        // even if not on ground)
        // OPTIMIZE: tablify angle
        // Note: a LUT allows for effects
        //  like a ramp with low health.
        if let Some(ref mut mobj) = self.mobj {
            let x = mobj.obj.momxy.x();
            let y = mobj.obj.momxy.y();
            self.bob = x * x + y * y;

            // Reduce precision
            self.bob = (self.bob as i32 >> 2) as f32;

            if self.bob > MAXBOB {
                self.bob = MAXBOB;
            }

            // TODO: if ((player->cheats & CF_NOMOMENTUM) || !onground)
            if !self.onground {
                self.viewz = mobj.obj.z + VIEWHEIGHT;

                if self.viewz > mobj.obj.ceilingz - 4.0 {
                    self.viewz = mobj.obj.ceilingz - 4.0;
                }

                self.viewz = mobj.obj.z + self.viewheight;
            }

            // Need to shunt finesine left by 13 bits?
            // Removed the shifts and division from `angle = (FINEANGLES / 20 * leveltime) & FINEMASK;`
            let angle = ((3350528u32.overflowing_mul(level_time).0) & 67100672)
                as f32
                * 8.38190317e-8;
            let bob = self.bob / 2.0 * angle.cos(); // not sine!

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
                    // player->deltaviewheight += FRACUNIT / 4;
                    self.deltaviewheight += 0.25;
                    if self.deltaviewheight <= 0.0 {
                        self.deltaviewheight = 1.0;
                    }
                }
            }

            self.viewz = mobj.obj.z + self.viewheight + bob;

            // if (player->viewz > player->mo->ceilingz - 4 * FRACUNIT)
            if self.viewz > mobj.obj.ceilingz - 4.0 {
                self.viewz = mobj.obj.ceilingz - 4.0;
            }
        }
    }

    fn move_player(&mut self) {
        // TODO: Fix adjustments after fixing the tic timestep
        if self.cmd.angleturn != 0 {
            let a = bam_to_radian((self.cmd.angleturn as u32) << 16);
            self.mobj.as_mut().unwrap().obj.angle += a;
        }

        self.onground = if let Some(think) = self.mobj.as_ref() {
            think.obj.z <= think.obj.floorz
        } else {
            false
        };

        if self.cmd.forwardmove != 0 && self.onground {
            let angle = self.mobj.as_mut().unwrap().obj.angle;
            self.thrust(angle, self.cmd.forwardmove as i32 * 2048);
        }

        if self.cmd.sidemove != 0 && self.onground {
            let angle = self.mobj.as_mut().unwrap().obj.angle;
            self.thrust(angle - FRAC_PI_2, self.cmd.sidemove as i32 * 2048);
        }

        if self.cmd.forwardmove != 0 || self.cmd.sidemove != 0 {
            if let Some(ref thinker) = self.mobj {
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

        if let Some(ref mut mo) = self.mobj {
            mo.think(level); // Player own the thinker, so make it think here
        }
        false
    }
}
