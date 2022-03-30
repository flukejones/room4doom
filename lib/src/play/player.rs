use std::f32::consts::FRAC_PI_2;

use glam::Vec2;
use log::{debug, info};

use super::{
    map_object::MapObject,
    player_sprite::PspDef,
    utilities::{bam_to_radian, fixed_to_float, p_random, MAXHEALTH, VIEWHEIGHT},
};

use crate::{
    angle::Angle,
    doom_def::{AmmoType, Card, PowerType, WeaponType, MAXPLAYERS, MAX_AMMO},
    info::{SpriteNum, StateNum},
    level::Level,
    play::map_object::MobjFlag,
    tic_cmd::{TicCmd, TIC_CMD_BUTTONS},
};

/// 16 pixels of bob
const MAX_BOB: f32 = 16.0; // 0x100000;

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
pub enum PlayerCheat {
    /// No clipping, walk through barriers.
    Noclip = 1,
    /// No damage, no health loss.
    Godmode = 2,
    /// Not really a cheat, just a debug aid.
    NoMomentum = 4,
}

/// INTERMISSION
/// Structure passed e.g. to WI_Start(wb)
#[derive(Default)]
pub struct WBPlayerStruct {
    /// whether the player is in game
    pub inn: bool,
    // Player stats, kills, collected items etc.
    pub skills: i32,
    pub sitems: i32,
    pub ssecret: i32,
    pub stime: u32,
    pub frags: [i32; 4],
    /// current score on entry, modified on return
    pub score: i32,
}

/// parms for world level / intermission
#[derive(Default)]
pub struct WBStartStruct {
    /// episode # (0-2)
    pub epsd: i32,
    /// if true, splash the secret level
    pub didsecret: bool,
    /// previous and next levels, origin 0
    pub last: i32,
    pub next: i32,
    pub maxkills: i32,
    pub maxitems: i32,
    pub maxsecret: i32,
    pub maxfrags: i32,
    /// the par time
    pub partime: i32,
    /// index of this player in game
    pub pnum: i32,
    pub plyr: [WBPlayerStruct; MAXPLAYERS as usize],
}

/// player_t
pub struct Player {
    pub mobj: Option<*mut MapObject>,
    pub player_state: PlayerState,
    pub cmd: TicCmd,

    /// Determine POV,
    ///  including viewpoint bobbing during movement.
    /// Focal origin above r.z
    pub viewz: f32,
    /// Base height above floor for viewz.
    pub viewheight: f32,
    /// Bob/squat speed.
    pub deltaviewheight: f32,
    /// bounded/scaled total momentum.
    pub bob: f32,
    pub onground: bool,

    /// This is only used between levels,
    /// mo->health is used during levels.
    pub health: i32,
    pub armorpoints: i32,
    /// Armor type is 0-2.
    pub armortype: i32,

    /// Power ups. invinc and invis are tic counters.
    pub powers: [i32; PowerType::NUMPOWERS as usize],
    pub cards: [bool; Card::NUMCARDS as usize],
    pub backpack: bool,

    /// Frags, kills of other players.
    pub frags: [i32; MAXPLAYERS as usize],
    pub readyweapon: WeaponType,

    /// Is wp_nochange if not changing.
    pendingweapon: WeaponType,

    pub weaponowned: [bool; WeaponType::NUMWEAPONS as usize],
    pub ammo: [u32; AmmoType::NUMAMMO as usize],
    pub maxammo: [u32; AmmoType::NUMAMMO as usize],

    /// True if button down last tic.
    pub attackdown: bool,
    pub usedown: bool,

    /// Bit flags, for cheats and debug.
    /// See cheat_t, above.
    pub cheats: u32,

    /// Refired shots are less accurate.
    pub refire: i32,

    /// For intermission stats.
    pub killcount: i32,
    pub itemcount: i32,
    pub secretcount: i32,

    /// Hint messages.
    pub message: Option<String>,

    /// For screen flashing (red or bright).
    pub damagecount: i32,
    pub bonuscount: i32,

    // Who did damage (NULL for floors/ceilings).
    pub attacker: Option<*mut MapObject>,
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
    pub didsecret: bool,

    // Custom option
    pub head_bob: bool,
}

impl Default for Player {
    fn default() -> Self {
        Player::new()
    }
}

impl Player {
    pub const fn new() -> Player {
        Player {
            viewz: 0.0,
            mobj: None,
            attacker: None,

            viewheight: 41.0,
            deltaviewheight: 1.0,
            bob: 1.0,
            onground: true,
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
            weaponowned: [false; WeaponType::NUMWEAPONS as usize],

            player_state: PlayerState::PstReborn,
            cmd: TicCmd::new(),

            head_bob: true,

            psprites: [
                PspDef {
                    state: None,
                    tics: 1,
                    sx: 0.0,
                    sy: 0.0,
                },
                PspDef {
                    state: None,
                    tics: 1,
                    sx: 0.0,
                    sy: 0.0,
                },
            ],
        }
    }
    // TODO: needs p_pspr.c, p_inter.c

    /// Doom function `G_PlayerFinishLevel`, mostly.
    pub fn finish_level(&mut self) {
        for card in self.cards.iter_mut() {
            *card = false;
        }
        for power in self.powers.iter_mut() {
            *power = 0;
        }

        self.extralight = 0;
        self.fixedcolormap = 0;
        self.damagecount = 0;
        self.bonuscount = 0;

        if let Some(mobj) = self.mobj {
            unsafe {
                (*mobj).flags &= !(MobjFlag::SHADOW as u32);
            }
        }

        info!("Reset level items and powers for player");
    }

    /// Doom function `G_PlayerReborn`, mostly.
    pub fn reborn(&mut self) {
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
        self.maxammo.copy_from_slice(&MAX_AMMO);
    }

    /// P_Thrust
    /// Moves the given origin along a given angle.
    fn thrust(&mut self, angle: Angle, mv: i32) {
        // mv is in a fixed float format, we need to convert it
        let mv = fixed_to_float(mv);
        let x = mv as f32 * angle.cos();
        let y = mv as f32 * angle.sin();
        let mxy = Vec2::new(x, y);

        if let Some(mobj) = self.mobj {
            unsafe {
                (*mobj).momxy += mxy;
            }
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
        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &mut *mobj };
            let x = mobj.momxy.x();
            let y = mobj.momxy.y();
            self.bob = x * x + y * y;

            // Reduce precision
            self.bob = (self.bob as i32 >> 2) as f32;

            if self.bob > MAX_BOB {
                self.bob = MAX_BOB;
            }

            // TODO: if ((player->cheats & CF_NOMOMENTUM) || !onground)
            if !self.onground {
                self.viewz = mobj.z + VIEWHEIGHT;
                if self.viewz > mobj.ceilingz - 4.0 {
                    self.viewz = mobj.ceilingz - 4.0;
                }

                self.viewz = mobj.z + self.viewheight;
            }

            // Need to shunt finesine left by 13 bits?
            // Removed the shifts and division from `angle = (FINEANGLES / 20 * leveltime) & FINEMASK;`
            let mut bob = 0.0;
            if self.head_bob {
                let angle =
                    ((3350528u32.overflowing_mul(level_time).0) & 67100672) as f32 * 8.381_903e-8;
                bob = self.bob / 2.0 * angle.cos(); // not sine!
            }

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
                    self.deltaviewheight += 0.25;
                    if self.deltaviewheight <= 0.0 {
                        self.deltaviewheight = 1.0;
                    }
                }
            }

            self.viewz = mobj.z + self.viewheight + bob;

            if self.viewz > mobj.ceilingz - 4.0 {
                self.viewz = mobj.ceilingz - 4.0;
            }
            self.viewz = self.viewz.round();
        }
    }

    /// P_MovePlayer
    fn move_player(&mut self) {
        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &mut *mobj };

            // TODO: Fix adjustments after fixing the tic timestep
            if self.cmd.angleturn != 0 {
                let a = bam_to_radian((self.cmd.angleturn as u32) << 16);
                mobj.angle += a;
            }

            self.onground = mobj.z <= mobj.floorz;

            if self.cmd.forwardmove != 0 && self.onground {
                let angle = mobj.angle;
                self.thrust(angle, self.cmd.forwardmove as i32 * 2048);
            }

            if self.cmd.sidemove != 0 && self.onground {
                let angle = mobj.angle;
                self.thrust(angle - FRAC_PI_2, self.cmd.sidemove as i32 * 2048);
            }

            if (self.cmd.forwardmove != 0 || self.cmd.sidemove != 0)
                && mobj.state.sprite as i32 == SpriteNum::SPR_PLAY as i32
            {
                mobj.set_state(StateNum::S_PLAY_RUN1);
            }
        }
    }

    /// Doom function name `P_PlayerInSpecialSector`
    fn in_special_sector(&mut self, level: &mut Level) {
        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &mut *mobj };
            let mut sector = unsafe { (*mobj.subsector).sector.clone() };

            if mobj.z != sector.floorheight {
                return;
            }

            match sector.special {
                // HELLSLIME DAMAGE
                5 => {
                    if self.powers[PowerType::pw_ironfeet as usize] == 0
                        && level.level_time & 0x1f == 0
                    {
                        debug!("Hell-slime damage!");
                        mobj.p_take_damage(None, None, false, 10);
                    }
                }
                // NUKAGE DAMAGE
                7 => {
                    if self.powers[PowerType::pw_ironfeet as usize] == 0
                        && level.level_time & 0x1f == 0
                    {
                        debug!("Nukage damage!");
                        mobj.p_take_damage(None, None, false, 5);
                    }
                }
                // SUPER HELLSLIME DAMAGE | STROBE HURT
                16 | 4 => {
                    if (self.powers[PowerType::pw_ironfeet as usize] == 0 || p_random() < 5)
                        && level.level_time & 0x1f == 0
                    {
                        debug!("Super hell-slime damage!");
                        mobj.p_take_damage(None, None, false, 20);
                    }
                }
                // SECRET SECTOR
                9 => {
                    info!("Found secret!");
                    self.secretcount += 1;
                    sector.special = 0;
                }
                // EXIT SUPER DAMAGE! (for E1M8 finale)
                11 => {
                    self.cheats &= !(PlayerCheat::Godmode as u32);
                    if level.level_time & 0x1f == 0 {
                        debug!("End of episode damage!");
                        mobj.p_take_damage(None, None, false, 20);
                    }
                    if self.health <= 10 {
                        level.do_exit_level();
                    }
                }
                _ => {}
            }
        }
    }
}

/// P_PlayerThink
/// The Doom source has the thinker in a specific location in the object structs
/// which enables a cast to t_thinker. We can't do that in rust so need to use the trait.
impl Player {
    pub fn think(&mut self, level: &mut Level) -> bool {
        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &mut *mobj };
            if self.cheats & PlayerCheat::Noclip as u32 != 0 {
                mobj.flags |= MobjFlag::NOCLIP as u32;
            } else {
                mobj.flags &= !(MobjFlag::NOCLIP as u32);
            }
        }

        if self.player_state == PlayerState::PstDead {
            self.death_think(level);
            return false;
        }

        // TODO: not feature complete with P_PlayerThink
        if let Some(mut mobj) = self.mobj {
            unsafe {
                if (*mobj).reactiontime > 0 {
                    (*mobj).reactiontime -= 1;
                } else {
                    self.move_player();
                }
            }
        }
        self.calculate_height(level.level_time);

        self.in_special_sector(level);

        if self.cmd.buttons & TIC_CMD_BUTTONS.bt_use != 0 {
            if !self.usedown {
                self.usedown = true;
                if let Some(mobj) = self.mobj {
                    unsafe {
                        (*mobj).use_lines();
                    }
                }
            }
        } else {
            self.usedown = false;
        }

        if self.cmd.buttons & TIC_CMD_BUTTONS.bt_attack != 0 {
            if !self.attackdown {
                self.attackdown = true;
                if let Some(_mobj) = self.mobj {
                    println!("PEWPEW!!");
                }
            }
        } else {
            self.attackdown = false;
        }

        false
    }

    pub fn death_think(&mut self, level: &mut Level) {
        if let Some(mobj) = self.mobj {
            unsafe {
                if self.viewz >= (*mobj).floorz + 3.0 {
                    self.viewz -= 1.0;
                }
                if self.viewz == 3.0 {
                    self.viewz -= 1.0;
                    info!("You died! Press use-button to respawn");
                }
            }
        }

        if self.cmd.buttons & TIC_CMD_BUTTONS.bt_use != 0 {
            if !self.usedown {
                self.usedown = true;
                self.player_state = PlayerState::PstReborn;
            }
        } else {
            self.usedown = false;
        }
    }
}
