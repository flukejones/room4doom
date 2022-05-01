use std::f32::consts::FRAC_PI_2;

use glam::Vec2;
use log::{debug, error, info};

use super::{
    enemy::noise_alert,
    mobj::{MapObject, BONUSADD},
    player_sprite::{PspDef, WEAPONBOTTOM},
    utilities::{bam_to_radian, fixed_to_float, p_random, point_to_angle_2, MAXHEALTH, VIEWHEIGHT},
};

use crate::{
    angle::Angle,
    doom_def::{
        AmmoType, Card, PowerDuration, PowerType, WeaponType, BFGCELLS, CLIP_AMMO, MAXPLAYERS,
        MAX_AMMO, WEAPON_INFO,
    },
    info::{ActionF, SpriteNum, StateNum, STATES},
    level::Level,
    pic::INVERSECOLORMAP,
    play::mobj::MapObjectFlag,
    tic_cmd::{TicCmd, TIC_CMD_BUTTONS},
    GameMode, Skill,
};

/// 16 pixels of bob
const MAX_BOB: f32 = 16.0; // 0x100000;
const ANG5: f32 = 0.08726646; //5f32.to_radians();

/// Overlay psprites are scaled shapes
/// drawn directly on the view screen,
/// coordinates are given for a 320*200 view screen.
///
/// From P_PSPR
pub enum PsprNum {
    Weapon,
    Flash,
    NumPSprites,
}

//// Player states.
#[derive(Debug, PartialEq)]
pub enum PlayerState {
    /// Playing or camping.
    Live,
    /// Dead on the ground, view follows killer.
    Dead,
    /// Ready to restart/respawn???
    Reborn,
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
    pub(crate) deltaviewheight: f32,
    /// bounded/scaled total momentum.
    pub(crate) bob: f32,
    pub(crate) onground: bool,

    /// This is only used between levels,
    /// mo->health is used during levels.
    pub health: i32,
    pub armorpoints: i32,
    /// Armor type is 0-2.
    // TODO: make enum
    pub armortype: i32,

    /// Power ups. invinc and invis are tic counters.
    pub(crate) powers: [i32; PowerType::NumPowers as usize],
    pub cards: [bool; Card::NumCards as usize],
    pub(crate) backpack: bool,

    /// Frags, kills of other players.
    pub frags: [i32; MAXPLAYERS as usize],
    pub readyweapon: WeaponType,

    /// Is wp_nochange if not changing.
    pub pendingweapon: WeaponType,

    pub weaponowned: [bool; WeaponType::NumWeapons as usize],
    pub ammo: [u32; AmmoType::NumAmmo as usize],
    pub maxammo: [u32; AmmoType::NumAmmo as usize],

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
    pub message: Option<&'static str>,

    /// For screen flashing (red or bright).
    pub damagecount: i32,
    pub bonuscount: i32,

    // Who did damage (NULL for floors/ceilings).
    pub(crate) attacker: Option<*mut MapObject>,
    /// So gun flashes light up areas.
    pub extralight: i32,

    /// Can be set to REDCOLORMAP for pain, etc.
    /// 0 = off.
    pub fixedcolormap: i32,

    /// Player skin colorshift,
    ///  0-3 for which color to draw player.
    colormap: i32,

    /// Overlay view sprites (gun, etc).
    pub psprites: [PspDef; PsprNum::NumPSprites as usize],

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
    pub fn new() -> Player {
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
            ammo: [0; AmmoType::NumAmmo as usize],
            maxammo: [0; AmmoType::NumAmmo as usize],
            powers: [0; PowerType::NumPowers as usize],
            cards: [false; Card::NumCards as usize],
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
            readyweapon: WeaponType::NoChange,
            pendingweapon: WeaponType::Pistol,
            weaponowned: [false; WeaponType::NumWeapons as usize],

            player_state: PlayerState::Reborn,
            cmd: TicCmd::new(),

            head_bob: true,

            psprites: [
                PspDef {
                    state: Some(&STATES[StateNum::S_PISTOLUP as usize]),
                    tics: 1,
                    sx: 0.0,
                    sy: WEAPONBOTTOM,
                },
                PspDef {
                    state: Some(&STATES[StateNum::S_PISTOLFLASH as usize]),
                    tics: 1,
                    sx: 0.0,
                    sy: WEAPONBOTTOM,
                },
            ],
        }
    }

    pub fn mobj_unchecked(&self) -> &MapObject {
        unsafe { &*self.mobj.unwrap_unchecked() }
    }

    pub fn mobj_mut_unchecked(&mut self) -> &mut MapObject {
        unsafe { &mut *self.mobj.unwrap_unchecked() }
    }

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
        self.mobj_mut_unchecked().flags &= !(MapObjectFlag::Shadow as u32);

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
        self.attackdown = true;
        self.player_state = PlayerState::Live;
        self.health = MAXHEALTH;
        self.readyweapon = WeaponType::Pistol;
        self.pendingweapon = WeaponType::NoChange;
        self.weaponowned[WeaponType::Fist as usize] = true;
        self.weaponowned[WeaponType::Pistol as usize] = true;
        self.ammo[AmmoType::Clip as usize] = 50;
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
        self.mobj_mut_unchecked().momxy += mxy;
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
            let x = mobj.momxy.x;
            let y = mobj.momxy.y;
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
            if self.player_state == PlayerState::Live {
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
            self.viewz = self.viewz.floor();
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

    /// Called every tic by player thinking routine to update sprites and states
    ///
    /// Doom function name `P_MovePsprites`
    fn move_player_sprites(&mut self) {
        let psps = unsafe { &mut *(&mut self.psprites as *mut [PspDef]) };
        for (i, psp) in psps.iter_mut().enumerate() {
            if let Some(state) = psp.state {
                // a -1 tic count never changes
                if psp.tics != -1 {
                    psp.tics -= 1;
                    if psp.tics == 0 {
                        self.set_psprite(i, state.next_state);
                    }
                }
            }
        }
        self.psprites[PsprNum::Flash as usize].sx = self.psprites[PsprNum::Weapon as usize].sx;
        self.psprites[PsprNum::Flash as usize].sy = self.psprites[PsprNum::Weapon as usize].sy;
    }

    pub(crate) fn set_psprite(&mut self, position: usize, mut state_num: StateNum) {
        loop {
            if state_num == StateNum::S_NULL {
                // object removed itself
                self.psprites[position].state = None;
                break;
            }

            let state = &STATES[state_num as usize];
            self.psprites[position].state = Some(state);
            self.psprites[position].tics = state.tics;

            if state.misc1 != 0 {
                self.psprites[position].sx = fixed_to_float(state.misc1);
                self.psprites[position].sy = fixed_to_float(state.misc2);
            }

            if let ActionF::Player(func) = state.action {
                let psps = unsafe { &mut *(&mut self.psprites[position] as *mut PspDef) };
                func(self, psps);
                if self.psprites[position].state.is_none() {
                    break;
                }
            }

            state_num = if let Some(state) = self.psprites[position].state {
                state.next_state
            } else {
                StateNum::S_NULL
            };

            if self.psprites[position].tics != 0 {
                break;
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
                    if self.powers[PowerType::IronFeet as usize] == 0
                        && level.level_time & 0x1f == 0
                    {
                        debug!("Hell-slime damage!");
                        mobj.p_take_damage(None, None, false, 10);
                    }
                }
                // NUKAGE DAMAGE
                7 => {
                    if self.powers[PowerType::IronFeet as usize] == 0
                        && level.level_time & 0x1f == 0
                    {
                        debug!("Nukage damage!");
                        mobj.p_take_damage(None, None, false, 5);
                    }
                }
                // SUPER HELLSLIME DAMAGE | STROBE HURT
                16 | 4 => {
                    if (self.powers[PowerType::IronFeet as usize] == 0 || p_random() < 5)
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

    pub(crate) fn give_ammo(&mut self, ammo: AmmoType, mut num: u32, skill: Skill) -> bool {
        if ammo == AmmoType::NoAmmo {
            return false;
        }
        if ammo == AmmoType::NumAmmo {
            error!("Tried to give AmmoType::NumAmmo");
            return false;
        }

        if self.ammo[ammo as usize] == self.maxammo[ammo as usize] {
            return false;
        }

        if num != 0 {
            num *= CLIP_AMMO[ammo as usize];
        } else {
            num = CLIP_AMMO[ammo as usize] / 2;
        }

        if skill == Skill::Baby || skill == Skill::Nightmare {
            // Double ammo for trainer mode + nightmare
            num <<= 1;
        }

        let old_ammo = self.ammo[ammo as usize];
        self.ammo[ammo as usize] += num;
        if self.ammo[ammo as usize] > self.maxammo[ammo as usize] {
            self.ammo[ammo as usize] = self.maxammo[ammo as usize];
        }

        // If non zero ammo, don't change up weapons, player was lower on purpose.
        if old_ammo != 0 {
            return true;
        }

        match ammo {
            AmmoType::Clip => {
                if self.readyweapon == WeaponType::Fist {
                    if self.weaponowned[WeaponType::Chaingun as usize] {
                        self.pendingweapon = WeaponType::Chaingun;
                    } else {
                        self.pendingweapon = WeaponType::Pistol;
                    }
                }
            }
            AmmoType::Shell => {
                if (self.readyweapon == WeaponType::Fist
                    || self.pendingweapon == WeaponType::Pistol)
                    && self.weaponowned[WeaponType::Shotgun as usize]
                {
                    self.pendingweapon = WeaponType::Shotgun;
                }
            }
            AmmoType::Cell => {
                if (self.readyweapon == WeaponType::Fist
                    || self.pendingweapon == WeaponType::Pistol)
                    && self.weaponowned[WeaponType::Plasma as usize]
                {
                    self.pendingweapon = WeaponType::Plasma;
                }
            }
            AmmoType::Missile => {
                if self.readyweapon == WeaponType::Fist
                    && self.weaponowned[WeaponType::Missile as usize]
                {
                    self.pendingweapon = WeaponType::Missile;
                }
            }
            _ => {}
        }
        true
    }

    pub(crate) fn give_weapon(&mut self, weapon: WeaponType, dropped: bool, skill: Skill) -> bool {
        let mut gave_ammo = false;
        let mut gave_weapon = false;
        // TODO: if (netgame && (deathmatch != 2) && !dropped) {
        let ammo = WEAPON_INFO[weapon as usize].ammo;
        if ammo != AmmoType::NoAmmo {
            if dropped {
                gave_ammo = self.give_ammo(ammo, 1, skill);
            } else {
                gave_ammo = self.give_ammo(ammo, 2, skill);
            }
        }

        if !self.weaponowned[weapon as usize] {
            gave_weapon = true;
            self.weaponowned[weapon as usize] = true;
            self.pendingweapon = weapon;
        }

        gave_ammo || gave_weapon
    }

    pub(crate) fn give_armour(&mut self, armour: i32) -> bool {
        let hits = armour * 100;
        if self.armorpoints >= hits {
            return false;
        }

        self.armortype = armour;
        self.armorpoints = hits;
        true
    }

    pub(crate) fn give_key(&mut self, card: Card) {
        if self.cards[card as usize] {
            return;
        }
        self.bonuscount += BONUSADD;
        self.cards[card as usize] = true;
    }

    pub(crate) fn give_body(&mut self, num: i32) -> bool {
        if self.health >= MAXHEALTH {
            return false;
        }

        self.health += num;
        if self.health > MAXHEALTH {
            self.health = MAXHEALTH;
        }

        true
    }

    pub(crate) fn give_power(&mut self, power: PowerType) -> bool {
        match power {
            PowerType::Invulnerability => {
                self.powers[power as usize] = PowerDuration::Invulnerability as i32;
                return true;
            }
            PowerType::Strength => {
                self.give_body(100);
                self.powers[power as usize] = 1;
                return true;
            }
            PowerType::Invisibility => {
                self.powers[power as usize] = PowerDuration::Invisibility as i32;
                return true;
            }
            PowerType::IronFeet => {
                self.powers[power as usize] = PowerDuration::IronFeet as i32;
                return true;
            }
            PowerType::Infrared => {
                self.powers[power as usize] = PowerDuration::Infrared as i32;
                return true;
            }
            _ => {}
        }

        if self.powers[power as usize] != 0 {
            return false; // Already got it
        }
        self.powers[power as usize] = 1;

        true
    }

    pub(crate) fn fire_weapon(&mut self) {
        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &mut *mobj };
            mobj.set_state(StateNum::S_PLAY_ATK1);
        }

        if !self.check_ammo() {
            return;
        }

        let new_state = WEAPON_INFO[self.readyweapon as usize].atkstate;
        self.set_psprite(PsprNum::Weapon as usize, new_state);
        noise_alert(self.mobj_mut_unchecked());
    }

    pub(crate) fn check_ammo(&mut self) -> bool {
        let ammo = &WEAPON_INFO[self.readyweapon as usize].ammo;
        // Minimum for one shot varies with weapon
        let count = if self.readyweapon == WeaponType::BFG {
            BFGCELLS
        } else if self.readyweapon == WeaponType::SuperShotgun {
            2
        } else {
            1
        };

        // Punch and chainsaw don't need ammo.
        if *ammo == AmmoType::NoAmmo || self.ammo[*ammo as usize] >= count {
            return true;
        }

        // Out of ammo so pick a new weapon
        loop {
            if self.weaponowned[WeaponType::Plasma as usize]
                && self.ammo[AmmoType::Cell as usize] != 0
            // TODO: && (gamemode != shareware)
            {
                self.pendingweapon = WeaponType::Plasma
            } else if self.weaponowned[WeaponType::SuperShotgun as usize]
                && self.ammo[AmmoType::Shell as usize] > 2
            // TODO: && (gamemode == commercial)
            {
                self.pendingweapon = WeaponType::SuperShotgun
            } else if self.weaponowned[WeaponType::Chaingun as usize]
                && self.ammo[AmmoType::Clip as usize] != 0
            {
                self.pendingweapon = WeaponType::Chaingun
            } else if self.weaponowned[WeaponType::Shotgun as usize]
                && self.ammo[AmmoType::Shell as usize] != 0
            {
                self.pendingweapon = WeaponType::Shotgun
            } else if self.ammo[AmmoType::Clip as usize] != 0 {
                self.pendingweapon = WeaponType::Pistol
            } else if self.weaponowned[WeaponType::Chainsaw as usize] {
                self.pendingweapon = WeaponType::Chainsaw
            } else if self.weaponowned[WeaponType::Missile as usize]
                && self.ammo[AmmoType::Missile as usize] != 0
            {
                self.pendingweapon = WeaponType::Missile
            } else if self.weaponowned[WeaponType::BFG as usize]
                && self.ammo[AmmoType::Cell as usize] >= 40
            // TODO: && (gamemode != shareware)
            {
                self.pendingweapon = WeaponType::BFG
            } else {
                self.pendingweapon = WeaponType::Fist
            }

            if self.pendingweapon != WeaponType::NoChange {
                break;
            }
        }

        self.set_psprite(
            PsprNum::Weapon as usize,
            WEAPON_INFO[self.readyweapon as usize].downstate,
        );

        false
    }

    pub(crate) fn bring_up_weapon(&mut self) {
        if self.pendingweapon == WeaponType::NoChange {
            self.pendingweapon = self.readyweapon;
        }
        if self.pendingweapon == WeaponType::Chainsaw {
            self.pendingweapon = self.readyweapon;
            // TODO: S_StartSound(player->mo, sfx_sawup);
        }

        let new_state = WEAPON_INFO[self.pendingweapon as usize].upstate;
        self.pendingweapon = WeaponType::NoChange;
        self.psprites[PsprNum::Weapon as usize].sy = WEAPONBOTTOM;

        self.set_psprite(PsprNum::Weapon as usize, new_state);
    }

    /// Check for mobj and set state of it
    pub(crate) fn set_mobj_state(&mut self, state: StateNum) {
        self.mobj_mut_unchecked().set_state(state);
    }

    pub(crate) fn subtract_readyweapon_ammo(&mut self, num: u32) {
        if self.ammo[WEAPON_INFO[self.readyweapon as usize].ammo as usize] != 0 {
            self.ammo[WEAPON_INFO[self.readyweapon as usize].ammo as usize] -= num;
        }
    }

    // pub(crate) fn get_mobj_angle(&mut self) -> Angle {
    //     unsafe { (*(self.mobj.unwrap())).angle }
    // }

    // pub(crate) fn get_mobj_xy(&mut self) -> Vec2 {
    //     unsafe { (*(self.mobj.unwrap())).xy }
    // }

    // pub(crate) fn mobj_aim_line_attack(&self, distance: f32, bsp_trace: &mut BSPTrace) -> Option<AimResult>{
    //     unsafe { (*(self.mobj.unwrap())).aim_line_attack(distance, bsp_trace) }
    // }
}

/// P_PlayerThink
/// The Doom source has the thinker in a specific location in the object structs
/// which enables a cast to t_thinker. We can't do that in rust so need to use the trait.
impl Player {
    pub fn think(&mut self, level: &mut Level) -> bool {
        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &mut *mobj };
            if self.cheats & PlayerCheat::Noclip as u32 != 0 {
                mobj.flags |= MapObjectFlag::NoClip as u32;
            } else {
                mobj.flags &= !(MapObjectFlag::NoClip as u32);
            }

            let cmd = &mut self.cmd;
            if mobj.flags & MapObjectFlag::JustAttacked as u32 != 0 {
                cmd.angleturn = 0;
                cmd.forwardmove = (0xc800 / 512) as i8;
                cmd.sidemove = 0;
                mobj.flags &= !(MapObjectFlag::JustAttacked as u32);
            }
        }

        if self.player_state == PlayerState::Dead {
            self.death_think(level);
            return false;
        }

        // TODO: not feature complete with P_PlayerThink
        if self.mobj_mut_unchecked().reactiontime > 0 {
            self.mobj_mut_unchecked().reactiontime -= 1;
        } else {
            self.move_player();
        }

        self.calculate_height(level.level_time);

        self.in_special_sector(level);

        if self.cmd.buttons & TIC_CMD_BUTTONS.bt_change != 0 {
            let new_weapon = (self.cmd.buttons & TIC_CMD_BUTTONS.bt_weaponmask)
                >> TIC_CMD_BUTTONS.bt_weaponshift;
            let mut new_weapon = WeaponType::from(new_weapon);

            if new_weapon == WeaponType::Fist
                && self.weaponowned[WeaponType::Chainsaw as usize]
                && !(self.readyweapon == WeaponType::Chainsaw
                    && self.powers[PowerType::Strength as usize] == 0)
            {
                new_weapon = WeaponType::Chainsaw;
            }

            if level.game_mode == GameMode::Commercial
                && new_weapon == WeaponType::Shotgun
                && self.weaponowned[WeaponType::SuperShotgun as usize]
                && self.readyweapon != WeaponType::SuperShotgun
            {
                new_weapon = WeaponType::SuperShotgun;
            }

            if self.weaponowned[new_weapon as usize] && new_weapon != self.readyweapon {
                // Do not go to plasma or BFG in shareware,
                //  even if cheated.
                if (new_weapon != WeaponType::Plasma && new_weapon != WeaponType::BFG)
                    || (level.game_mode != GameMode::Shareware)
                {
                    self.pendingweapon = new_weapon;
                }
            }
        }

        if self.cmd.buttons & TIC_CMD_BUTTONS.bt_use != 0 {
            if !self.usedown {
                self.usedown = true;
                self.mobj_mut_unchecked().use_lines();
            }
        } else {
            self.usedown = false;
        }

        self.move_player_sprites();

        // Powers and timers
        if self.powers[PowerType::Strength as usize] != 0 {
            // Strength counts up to diminish fade.
            self.powers[PowerType::Strength as usize] += 1;
        }

        if self.powers[PowerType::Invulnerability as usize] != 0 {
            self.powers[PowerType::Invulnerability as usize] -= 1;
        }

        if self.powers[PowerType::Infrared as usize] != 0 {
            self.powers[PowerType::Infrared as usize] -= 1;
        }

        if self.powers[PowerType::IronFeet as usize] != 0 {
            self.powers[PowerType::IronFeet as usize] -= 1;
        }

        if self.powers[PowerType::Invisibility as usize] != 0 {
            self.powers[PowerType::Invisibility as usize] -= 1;
            if self.powers[PowerType::Invisibility as usize] == 0 {
                self.mobj_mut_unchecked().flags &= !(MapObjectFlag::Shadow as u32);
            }
        }

        // Screen flashing, red, damage etc
        if self.damagecount != 0 {
            self.damagecount -= 1;
        }

        if self.bonuscount != 0 {
            self.bonuscount -= 1;
        }

        // Setting the colourmaps
        let invulnerability = self.powers[PowerType::Invulnerability as usize];
        let infrared = self.powers[PowerType::Infrared as usize];
        if invulnerability != 0 {
            if invulnerability > 4 * 32 || (invulnerability & 8 != 0) {
                self.fixedcolormap = INVERSECOLORMAP;
            } else {
                self.fixedcolormap = 0;
            }
        } else if infrared != 0 {
            if infrared > 4 * 32 || (infrared & 8 != 0) {
                self.fixedcolormap = 1; // almost fullbright
            } else {
                self.fixedcolormap = 0;
            }
        } else {
            self.fixedcolormap = 0;
        }

        false
    }

    /// Fall down and put weapon away on death
    ///
    /// Doom function name `P_DeathThink`
    pub fn death_think(&mut self, level: &mut Level) {
        self.move_player_sprites();

        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &mut *mobj };
            if self.viewheight >= 6.0 {
                self.viewheight -= 1.0;
            }
            if self.viewheight == 6.0 {
                info!("You died! Press use-button to respawn");
            }

            self.onground = mobj.z <= mobj.floorz;
            self.calculate_height(level.level_time);

            if let Some(attacker) = self.attacker {
                let attacker = unsafe { &mut *attacker };
                if !std::ptr::eq(mobj, attacker) {
                    let angle = point_to_angle_2(attacker.xy, mobj.xy);
                    let delta = mobj.angle.unit().angle_between(angle.unit());

                    if delta.abs() <= ANG5 {
                        mobj.angle = angle;
                        if self.damagecount > 0 {
                            self.damagecount -= 1;
                        }
                    } else if delta > -ANG5 {
                        mobj.angle += ANG5;
                    } else {
                        mobj.angle -= ANG5;
                    }
                }
            } else if self.damagecount > 0 {
                self.damagecount -= 1;
            }
        }

        if self.cmd.buttons & TIC_CMD_BUTTONS.bt_use != 0 {
            self.player_state = PlayerState::Reborn;
        }
    }
}
