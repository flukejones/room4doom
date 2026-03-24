use bitflags::bitflags;
use math::{ANG90, ANG180, Bam, FixedT};

use log::{debug, error, info};
use sound_common::SfxName;

use crate::doom_def::{
    AmmoType, BFGCELLS, CLIP_AMMO, Card, MAX_AMMO, MAXHEALTH, MAXPLAYERS, PowerDuration, PowerType, VIEWHEIGHT, WEAPON_INFO
};
use crate::info::{STATES, StateNum};
use crate::level::LevelState;
use crate::player_sprite::{PspDef, WEAPONBOTTOM};
use crate::thing::enemy::noise_alert;
use crate::thing::{BONUSADD, MapObjFlag, MapObject};
use game_config::tic_cmd::{TIC_CMD_BUTTONS, TicCmd};
use game_config::{GameMode, Skill, WeaponType};
use math::{Angle, fixed_to_float, p_random, r_point_to_angle};
use pic_data::INVERSECOLORMAP;

/// +/-90 degrees in signed BAM = +/-(i32::MAX / 2)
const MAX_PITCH_BAM: i32 = i32::MAX / 2;

/// OG Doom MAXBOB = 0x100000 (16 world units in 16.16 fixed-point)
const MAX_BOB: i32 = 0x100000;
/// OG Doom: ANG5 = 0x00E38E39 (5 degrees in BAM)
const ANG5_BAM: u32 = 0x00E3_8E39;

/// Snapshot of player visual state for rendering interpolation.
#[derive(Clone, Copy, Default)]
pub struct PrevRenderState {
    pub x: FixedT,
    pub y: FixedT,
    pub z: FixedT,
    pub viewz: FixedT,
    pub angle_bam: u32,
    pub lookdir: i32,
    pub psp_sx: [f32; 2],
    pub psp_sy: [f32; 2],
}

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

/// Player states.
#[derive(Debug, PartialEq, Eq)]
pub enum PlayerState {
    /// Playing or camping.
    Live,
    /// Dead on the ground, view follows killer.
    Dead,
    /// Ready to restart/respawn???
    Reborn,
}

bitflags! {
    /// Player internal flags, for cheats and debug.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct PlayerCheat: u32 {
        /// No clipping, walk through barriers.
        const Noclip = 1;
        /// No damage, no health loss.
        const Godmode = 2;
        /// Not really a cheat, just a debug aid.
        const NoMomentum = 4;
    }
}

/// INTERMISSION
/// Structure passed e.g. to WI_Start(wb)
#[derive(Default, Clone)]
pub struct WorldEndPlayerInfo {
    /// whether the player is in game-exe
    pub inn: bool,
    // Player stats, kills, collected items etc.
    /// skills
    pub total_kills: i32,
    /// sitems
    pub items_collected: i32,
    /// ssecret
    pub secrets_found: i32,
    /// stime
    pub level_time: u32,
    pub frags: [i32; 4],
    /// current score on entry, modified on return
    pub score: i32,
}

/// Contains the players current status such as attacking, loadout, health. This
/// is also used by the statusbar to show the player what their current status
/// is.
#[derive(Debug, Clone)]
pub struct PlayerStatus {
    /// True if button down last tic.
    pub attackdown: bool,
    pub usedown: bool,
    pub readyweapon: WeaponType,
    /// This is only used between levels,
    /// mo->health is used during levels.
    pub health: i32,
    pub armorpoints: i32,
    /// Armor type is 0-2.
    // TODO: make enum
    pub armortype: i32,
    pub cards: [bool; Card::NumCards as usize],
    pub weaponowned: [bool; WeaponType::NumWeapons as usize],
    pub ammo: [u32; AmmoType::NumAmmo as usize],
    pub maxammo: [u32; AmmoType::NumAmmo as usize],
    pub(crate) backpack: bool,
    /// Power ups. invinc and invis are tic counters.
    pub powers: [i32; PowerType::NumPowers as usize],
    /// For screen flashing (red or bright).
    pub damagecount: i32,
    pub bonuscount: i32,
    pub attacked_from: Angle,
    pub own_angle: Angle,
    pub attacked_angle_count: u32,
    /// Bit flags, for cheats and debug.
    /// See cheat_t, above.
    pub cheats: PlayerCheat,
}

impl Default for PlayerStatus {
    fn default() -> Self {
        let mut tmp = Self {
            attackdown: false,
            usedown: false,
            readyweapon: WeaponType::Pistol,
            health: MAXHEALTH,
            armorpoints: 0,
            armortype: 0,
            cards: Default::default(),
            weaponowned: Default::default(),
            ammo: Default::default(),
            maxammo: Default::default(),
            backpack: false,
            powers: [0; PowerType::NumPowers as usize],
            damagecount: 0,
            bonuscount: 0,
            attacked_from: Default::default(),
            own_angle: Default::default(),
            attacked_angle_count: 0,
            cheats: PlayerCheat::empty(),
        };
        tmp.ammo[AmmoType::Clip as usize] = 50;
        tmp.maxammo.copy_from_slice(&MAX_AMMO);
        tmp.weaponowned[WeaponType::Fist as usize] = true;
        tmp.weaponowned[WeaponType::Pistol as usize] = true;
        tmp
    }
}

/// player_t
pub struct Player {
    mobj: Option<*mut MapObject>,
    pub player_state: PlayerState,
    pub cmd: TicCmd,

    /// Determine POV,
    ///  including viewpoint bobbing during movement.
    /// Focal origin above r.z
    pub viewz: FixedT,
    /// Base height above floor for viewz.
    pub viewheight: FixedT,
    /// Bob/squat speed.
    pub(crate) deltaviewheight: FixedT,
    /// bounded/scaled total momentum (OG: fixed_t, after >>2).
    pub(crate) bob: FixedT,
    pub(crate) onground: bool,

    pub status: PlayerStatus,

    /// Frags, kills of other players.
    pub frags: [i32; MAXPLAYERS],

    /// Is wp_nochange if not changing.
    pub pendingweapon: WeaponType,

    /// Refired shots are less accurate.
    pub refire: i32,

    /// For intermission stats.
    pub total_kills: i32,
    pub items_collected: i32,
    pub secrets_found: i32,

    /// HUD messages.
    /// This doesn't need to be a queue as it's relatively impossible to
    /// do more than one thing in a single frame
    pub message: Option<&'static str>,

    // Who did damage (NULL for floors/ceilings).
    pub(crate) attacker: Option<*mut MapObject>,
    /// So gun flashes light up areas.
    pub extralight: usize,

    /// Can be set to REDCOLORMAP for pain, etc.
    /// 0 = off.
    pub fixedcolormap: i32,

    /// Player skin colorshift,
    ///  0-3 for which color to draw player.
    _colormap: i32,

    /// Overlay view sprites (gun, etc).
    pub psprites: [PspDef; PsprNum::NumPSprites as usize],

    /// True if secret level has been done.
    pub didsecret: bool,

    // Custom option
    pub head_bob: bool,
    /// Vertical look angle in signed BAM. Applied via `<< 16` like angleturn.
    pub lookdir: i32,
    /// Previous tic state for rendering interpolation.
    pub prev_render: PrevRenderState,
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    pub fn new() -> Self {
        Self {
            viewz: FixedT::ZERO,
            mobj: None,
            attacker: None,

            viewheight: 41.into(),
            deltaviewheight: 1.into(),
            bob: FixedT::ZERO,
            onground: true,
            status: PlayerStatus::default(),
            refire: 0,

            total_kills: 0,
            items_collected: 0,
            secrets_found: 0,

            message: None,

            _colormap: 0,
            didsecret: false,
            extralight: 0,
            fixedcolormap: 0,

            frags: [0; 4],
            pendingweapon: WeaponType::Pistol,

            player_state: PlayerState::Reborn,
            cmd: TicCmd::new(),

            head_bob: true,
            lookdir: 0,
            prev_render: PrevRenderState::default(),

            psprites: [
                PspDef {
                    state: Some(&STATES[StateNum::PISTOLUP as usize]),
                    tics: 1,
                    sx: 0.0,
                    sy: WEAPONBOTTOM,
                },
                PspDef {
                    state: Some(&STATES[StateNum::PISTOLFLASH as usize]),
                    tics: 1,
                    sx: 0.0,
                    sy: WEAPONBOTTOM,
                },
            ],
        }
    }

    pub fn mobj(&self) -> Option<&MapObject> {
        self.mobj.map(|m| unsafe { &*m })
    }

    pub fn mobj_mut(&mut self) -> Option<&mut MapObject> {
        self.mobj.map(|m| unsafe { &mut *m })
    }

    pub const fn mobj_raw(&mut self) -> Option<*mut MapObject> {
        self.mobj
    }

    pub const fn set_mobj(&mut self, mobj: *mut MapObject) {
        self.mobj = Some(mobj);
    }

    /// Unchecked access to the raw `MapObject` pointer cast to ref
    ///
    /// # Safety
    /// The players `MapObject` *must* be valid and initialised.
    pub const unsafe fn mobj_unchecked(&self) -> &MapObject {
        unsafe { &*self.mobj.unwrap_unchecked() }
    }

    /// Unchecked access to the raw `MapObject` pointer cast to ref mut
    ///
    /// # Safety
    /// The players `MapObject` *must* be valid and initialised.
    pub const unsafe fn mobj_mut_unchecked(&mut self) -> &mut MapObject {
        unsafe { &mut *self.mobj.unwrap_unchecked() }
    }

    pub fn start_sound(&self, sfx: SfxName) {
        if let Some(mobj) = self.mobj() {
            unsafe {
                (*mobj.level).start_sound(
                    sfx,
                    mobj.x.to_f32(),
                    mobj.y.to_f32(),
                    self as *const Self as usize, /* pointer cast as a UID */
                )
            }
        }
    }

    /// Doom function `G_PlayerFinishLevel`, mostly.
    pub fn finish_level(&mut self) {
        for card in self.status.cards.iter_mut() {
            *card = false;
        }
        for power in self.status.powers.iter_mut() {
            *power = 0;
        }

        self.extralight = 0;
        self.fixedcolormap = 0;
        self.status.damagecount = 0;
        self.status.bonuscount = 0;
        if let Some(mobj) = self.mobj_mut() {
            mobj.flags.remove(MapObjFlag::Shadow);
        }

        info!("Reset level items and powers for player");
    }

    /// Doom function `G_PlayerReborn`, mostly.
    pub fn reborn(&mut self) {
        let kill_count = self.total_kills;
        let item_count = self.items_collected;
        let secret_count = self.secrets_found;

        *self = Self::default();
        self.total_kills = kill_count;
        self.items_collected = item_count;
        self.secrets_found = secret_count;

        self.status = PlayerStatus::default();
        self.status.attackdown = true;
        self.player_state = PlayerState::Live;
        self.status.readyweapon = WeaponType::Pistol;
        self.pendingweapon = WeaponType::NoChange;
    }

    /// P_Thrust
    /// Moves the given origin along a given angle.
    fn thrust(&mut self, angle: Angle<Bam>, mv: i32) {
        let mv = FixedT::from_fixed(mv);
        let bam = angle.to_bam();
        let cos = FixedT::cos_bam(bam);
        let sin = FixedT::sin_bam(bam);
        if let Some(mobj) = self.mobj_mut() {
            mobj.momx += mv.fixed_mul(cos);
            mobj.momy += mv.fixed_mul(sin);
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
            let x = mobj.momx;
            let y = mobj.momy;
            // OG: player->bob = FixedMul(momx,momx) + FixedMul(momy,momy); bob >>= 2;
            self.bob = ((x * x) + (y * y)).shr(2);

            let max_bob = FixedT::from_fixed(MAX_BOB);
            if self.bob > max_bob {
                self.bob = max_bob;
            }

            // TODO: if ((player->cheats & CF_NOMOMENTUM) || !onground)
            if !self.onground {
                self.viewz = mobj.z + self.viewheight;
                if self.viewz > mobj.ceilingz - 4 {
                    self.viewz = mobj.ceilingz - 4;
                }
                return;
            }

            // OG: angle = (FINEANGLES/20*leveltime) & FINEMASK
            let fine_angle = ((409 * level_time) & 8191) as usize;
            // OG: bob = FixedMul(player->bob/2, finesine[angle])
            let bob = self.bob.shr(1).fixed_mul(math::finesine(fine_angle));

            // move viewheight
            if self.player_state == PlayerState::Live {
                self.viewheight += self.deltaviewheight;

                if self.viewheight > VIEWHEIGHT {
                    self.viewheight = VIEWHEIGHT.into();
                    self.deltaviewheight = FixedT::ZERO;
                }

                // OG: VIEWHEIGHT/2 = (41*FRACUNIT)/2 = 1343488
                let half_viewheight = FixedT::from(VIEWHEIGHT).shr(1);
                if self.viewheight < half_viewheight {
                    self.viewheight = half_viewheight;
                    if self.deltaviewheight <= FixedT::ZERO {
                        self.deltaviewheight = FixedT::from_fixed(1); // OG: raw 1
                    }
                }

                // OG: if (player->deltaviewheight) — nonzero, not > 0
                if self.deltaviewheight != FixedT::ZERO {
                    // OG: player->deltaviewheight += FRACUNIT/4
                    self.deltaviewheight += FixedT::from_fixed(0x4000);
                    if self.deltaviewheight == FixedT::ZERO {
                        self.deltaviewheight = FixedT::from_fixed(1); // OG: raw 1
                    }
                }
            }

            self.viewz = mobj.z + self.viewheight + bob;

            if self.viewz > mobj.ceilingz - 4 {
                self.viewz = mobj.ceilingz - 4;
            }
        }
    }

    /// P_MovePlayer
    fn move_player(&mut self) {
        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &mut *mobj };

            // TODO: Fix adjustments after fixing the tic timestep
            if self.cmd.angleturn != 0 {
                mobj.angle += Angle::from_bam((self.cmd.angleturn as u32) << 16);
            }

            self.onground = mobj.z <= mobj.floorz;

            if self.cmd.forwardmove != 0 && self.onground {
                let angle = mobj.angle;
                self.thrust(angle, self.cmd.forwardmove as i32 * 2048);
            }

            if self.cmd.sidemove != 0 && self.onground {
                let bam = mobj.angle.to_bam().wrapping_sub(ANG90);
                self.thrust(Angle::from_bam(bam), self.cmd.sidemove as i32 * 2048);
            }

            if (self.cmd.forwardmove != 0 || self.cmd.sidemove != 0)
                && mobj.state_index() == StateNum::PLAY as usize
            {
                mobj.set_state(StateNum::PLAY_RUN1);
            }

            if self.cmd.lookdir != 0 {
                self.lookdir = (self.lookdir + ((self.cmd.lookdir as i32) << 16))
                    .clamp(-MAX_PITCH_BAM, MAX_PITCH_BAM);
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
            if state_num == StateNum::None {
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

            if let Some(func) = state.action.resolve_player() {
                let psps = unsafe { &mut *(&mut self.psprites[position] as *mut PspDef) };
                func(self, psps);
                if self.psprites[position].state.is_none() {
                    break;
                }
            }

            state_num = if let Some(state) = self.psprites[position].state {
                state.next_state
            } else {
                StateNum::None
            };

            if self.psprites[position].tics != 0 {
                break;
            }
        }
    }

    /// Doom function name `P_PlayerInSpecialSector`
    fn in_special_sector(&mut self, level: &mut LevelState) {
        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &mut *mobj };
            let mut sector = mobj.subsector.sector.clone();

            if mobj.z != FixedT::from_fixed(sector.floorheight.to_fixed_raw()) {
                return;
            }

            // BOOM: vanilla type in bits 0-4, damage in bits 5-6, secret in bit 7
            let vanilla_type = sector.special & 0x1F;
            let boom_damage = (sector.special & 0x60) >> 5;

            match vanilla_type {
                // HELLSLIME DAMAGE
                5 => {
                    if self.status.powers[PowerType::IronFeet as usize] == 0
                        && level.level_time & 0x1F == 0
                    {
                        debug!("Hell-slime damage!");
                        mobj.p_take_damage(None, None, 10);
                    }
                }
                // NUKAGE DAMAGE
                7 => {
                    if self.status.powers[PowerType::IronFeet as usize] == 0
                        && level.level_time & 0x1F == 0
                    {
                        debug!("Nukage damage!");
                        mobj.p_take_damage(None, None, 5);
                    }
                }
                // SUPER HELLSLIME DAMAGE | STROBE HURT
                16 | 4 => {
                    if (self.status.powers[PowerType::IronFeet as usize] == 0 || p_random() < 5)
                        && level.level_time & 0x1F == 0
                    {
                        debug!("Super hell-slime damage!");
                        mobj.p_take_damage(None, None, 20);
                    }
                }
                // SECRET SECTOR
                9 => {
                    self.secrets_found += 1;
                    sector.special = 0;
                }
                // EXIT SUPER DAMAGE! (for E1M8 finale)
                11 => {
                    self.status.cheats.remove(PlayerCheat::Godmode);
                    if level.level_time & 0x1F == 0 {
                        debug!("End of episode damage!");
                        mobj.p_take_damage(None, None, 20);
                    }
                    if self.status.health <= 10 {
                        level.do_completed();
                    }
                }
                _ => {}
            }

            // BOOM generalized damage (bits 5-6), applied independently of vanilla type
            if boom_damage > 0
                && vanilla_type != 5
                && vanilla_type != 7
                && vanilla_type != 4
                && vanilla_type != 16
            {
                if level.level_time & 0x1F == 0 {
                    let damage = match boom_damage {
                        1 => 5,
                        2 => 10,
                        3 => 20,
                        _ => 0,
                    };
                    if damage > 0
                        && (self.status.powers[PowerType::IronFeet as usize] == 0
                            || (boom_damage == 3 && p_random() < 5))
                    {
                        mobj.p_take_damage(None, None, damage);
                    }
                }
            }

            // BOOM secret (bit 7)
            if sector.special & 0x80 != 0 && vanilla_type != 9 {
                self.secrets_found += 1;
                sector.special &= !0x80;
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

        if self.status.ammo[ammo as usize] == self.status.maxammo[ammo as usize] {
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

        let old_ammo = self.status.ammo[ammo as usize];
        self.status.ammo[ammo as usize] += num;
        if self.status.ammo[ammo as usize] > self.status.maxammo[ammo as usize] {
            self.status.ammo[ammo as usize] = self.status.maxammo[ammo as usize];
        }

        // If non zero ammo, don't change up weapons, player was lower on purpose.
        if old_ammo != 0 {
            return true;
        }

        match ammo {
            AmmoType::Clip => {
                if self.status.readyweapon == WeaponType::Fist {
                    if self.status.weaponowned[WeaponType::Chaingun as usize] {
                        self.pendingweapon = WeaponType::Chaingun;
                    } else {
                        self.pendingweapon = WeaponType::Pistol;
                    }
                }
            }
            AmmoType::Shell => {
                if (self.status.readyweapon == WeaponType::Fist
                    || self.status.readyweapon == WeaponType::Pistol)
                    && self.status.weaponowned[WeaponType::Shotgun as usize]
                {
                    self.pendingweapon = WeaponType::Shotgun;
                }
            }
            AmmoType::Cell => {
                if (self.status.readyweapon == WeaponType::Fist
                    || self.status.readyweapon == WeaponType::Pistol)
                    && self.status.weaponowned[WeaponType::Plasma as usize]
                {
                    self.pendingweapon = WeaponType::Plasma;
                }
            }
            AmmoType::Missile => {
                if self.status.readyweapon == WeaponType::Fist
                    && self.status.weaponowned[WeaponType::Missile as usize]
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

        if !self.status.weaponowned[weapon as usize] {
            gave_weapon = true;
            self.status.weaponowned[weapon as usize] = true;
            self.pendingweapon = weapon;
        }

        gave_ammo || gave_weapon
    }

    pub(crate) fn give_armour(&mut self, armour: i32) -> bool {
        let hits = armour * 100;
        if self.status.armorpoints >= hits {
            return false;
        }

        self.status.armortype = armour;
        self.status.armorpoints = hits;
        true
    }

    pub(crate) fn give_key(&mut self, card: Card) {
        if self.status.cards[card as usize] {
            return;
        }
        self.status.bonuscount += BONUSADD;
        self.status.cards[card as usize] = true;
    }

    pub(crate) fn give_body(&mut self, num: i32) -> bool {
        if self.status.health >= MAXHEALTH {
            return false;
        }

        self.status.health += num;
        if self.status.health > MAXHEALTH {
            self.status.health = MAXHEALTH;
        }

        true
    }

    pub fn give_power(&mut self, power: PowerType) -> bool {
        if self.status.powers[power as usize] != 0 {
            return false; // Already got it
        }

        match power {
            PowerType::Invulnerability => {
                self.status.powers[power as usize] = PowerDuration::Invulnerability as i32;
            }
            PowerType::Strength => {
                self.give_body(100);
                self.status.powers[power as usize] = 1;
            }
            PowerType::Invisibility => {
                self.status.powers[power as usize] = PowerDuration::Invisibility as i32;
            }
            PowerType::IronFeet => {
                self.status.powers[power as usize] = PowerDuration::IronFeet as i32;
            }
            PowerType::Infrared => {
                self.status.powers[power as usize] = PowerDuration::Infrared as i32;
            }
            PowerType::NumPowers => {}
            PowerType::Allmap => {}
        }
        true
    }

    pub(crate) fn fire_weapon(&mut self) {
        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &mut *mobj };
            mobj.set_state(StateNum::PLAY_ATK1);
        }

        if !self.check_ammo() {
            return;
        }

        let new_state = WEAPON_INFO[self.status.readyweapon as usize].atkstate;
        self.set_psprite(PsprNum::Weapon as usize, new_state);
        if let Some(mobj) = self.mobj_mut() {
            noise_alert(mobj);
        }
    }

    pub(crate) fn check_ammo(&mut self) -> bool {
        let ammo = &WEAPON_INFO[self.status.readyweapon as usize].ammo;
        // Minimum for one shot varies with weapon
        let count = if self.status.readyweapon == WeaponType::BFG {
            BFGCELLS
        } else if self.status.readyweapon == WeaponType::SuperShotgun {
            2
        } else {
            1
        };

        // Punch and chainsaw don't need ammo.
        if *ammo == AmmoType::NoAmmo || self.status.ammo[*ammo as usize] >= count {
            return true;
        }

        // Out of ammo so pick a new weapon
        loop {
            if self.status.weaponowned[WeaponType::Plasma as usize]
                && self.status.ammo[AmmoType::Cell as usize] != 0
            // TODO: && (gamemode != shareware)
            {
                self.pendingweapon = WeaponType::Plasma
            } else if self.status.weaponowned[WeaponType::SuperShotgun as usize]
                && self.status.ammo[AmmoType::Shell as usize] > 2
            // TODO: && (gamemode == commercial)
            {
                self.pendingweapon = WeaponType::SuperShotgun
            } else if self.status.weaponowned[WeaponType::Chaingun as usize]
                && self.status.ammo[AmmoType::Clip as usize] != 0
            {
                self.pendingweapon = WeaponType::Chaingun
            } else if self.status.weaponowned[WeaponType::Shotgun as usize]
                && self.status.ammo[AmmoType::Shell as usize] != 0
            {
                self.pendingweapon = WeaponType::Shotgun
            } else if self.status.ammo[AmmoType::Clip as usize] != 0 {
                self.pendingweapon = WeaponType::Pistol
            } else if self.status.weaponowned[WeaponType::Chainsaw as usize] {
                self.pendingweapon = WeaponType::Chainsaw
            } else if self.status.weaponowned[WeaponType::Missile as usize]
                && self.status.ammo[AmmoType::Missile as usize] != 0
            {
                self.pendingweapon = WeaponType::Missile
            } else if self.status.weaponowned[WeaponType::BFG as usize]
                && self.status.ammo[AmmoType::Cell as usize] >= 40
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
            WEAPON_INFO[self.status.readyweapon as usize].downstate,
        );

        false
    }

    pub(crate) fn bring_up_weapon(&mut self) {
        if self.pendingweapon == WeaponType::NoChange {
            self.pendingweapon = self.status.readyweapon;
        }
        if self.pendingweapon == WeaponType::Chainsaw {
            self.pendingweapon = self.status.readyweapon;
            // TODO: StartSound(player->mo, sfx_sawup);
        }

        let new_state = WEAPON_INFO[self.pendingweapon as usize].upstate;
        self.pendingweapon = WeaponType::NoChange;
        self.psprites[PsprNum::Weapon as usize].sy = WEAPONBOTTOM;

        self.set_psprite(PsprNum::Weapon as usize, new_state);
    }

    /// Check for thing and set state of it
    pub(crate) fn set_mobj_state(&mut self, state: StateNum) {
        if let Some(mobj) = self.mobj_mut() {
            mobj.set_state(state);
        }
    }

    pub(crate) fn subtract_readyweapon_ammo(&mut self, num: u32) {
        if self.status.ammo[WEAPON_INFO[self.status.readyweapon as usize].ammo as usize] != 0 {
            self.status.ammo[WEAPON_INFO[self.status.readyweapon as usize].ammo as usize] -= num;
        }
    }

    /// P_DropWeapon
    pub(crate) fn drop_weapon(&mut self) {
        self.set_psprite(
            PsprNum::Weapon as usize,
            WEAPON_INFO[self.status.readyweapon as usize].downstate,
        );
    }
}

/// P_PlayerThink
/// The Doom source has the thinker in a specific location in the object structs
/// which enables a cast to t_thinker. We can't do that in rust so need to use
/// the trait.
impl Player {
    pub fn save_prev_render(&mut self) {
        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &*mobj };
            self.prev_render = PrevRenderState {
                x: mobj.x,
                y: mobj.y,
                z: mobj.z,
                viewz: self.viewz,
                angle_bam: mobj.angle.to_bam(),
                lookdir: self.lookdir,
                psp_sx: [self.psprites[0].sx, self.psprites[1].sx],
                psp_sy: [self.psprites[0].sy, self.psprites[1].sy],
            };
        }
    }

    pub fn think(&mut self, level: &mut LevelState) -> bool {
        self.save_prev_render();
        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &mut *mobj };
            if self.status.cheats.contains(PlayerCheat::Noclip) {
                mobj.flags.insert(MapObjFlag::Noclip);
            } else {
                mobj.flags.remove(MapObjFlag::Noclip);
            }

            let cmd = &mut self.cmd;
            if mobj.flags.contains(MapObjFlag::Justattacked) {
                cmd.angleturn = 0;
                cmd.forwardmove = (0xC800 / 512) as i8;
                cmd.sidemove = 0;
                mobj.flags.remove(MapObjFlag::Justattacked);
            }
        }

        if self.player_state == PlayerState::Dead {
            self.death_think(level);
            return false;
        }

        // TODO: not feature complete with P_PlayerThink
        if let Some(mobj) = self.mobj_mut() {
            if mobj.reactiontime > 0 {
                mobj.reactiontime -= 1;
            } else {
                self.move_player();
            }
        }

        self.calculate_height(level.level_time);

        self.in_special_sector(level);

        if self.cmd.buttons & TIC_CMD_BUTTONS.bt_change != 0 {
            let new_weapon = (self.cmd.buttons & TIC_CMD_BUTTONS.bt_weaponmask)
                >> TIC_CMD_BUTTONS.bt_weaponshift;
            let mut new_weapon = WeaponType::from(new_weapon);

            if new_weapon == WeaponType::Fist
                && self.status.weaponowned[WeaponType::Chainsaw as usize]
                && !(self.status.readyweapon == WeaponType::Chainsaw
                    && self.status.powers[PowerType::Strength as usize] == 0)
            {
                new_weapon = WeaponType::Chainsaw;
            }

            if level.game_mode == GameMode::Commercial
                && new_weapon == WeaponType::Shotgun
                && self.status.weaponowned[WeaponType::SuperShotgun as usize]
                && self.status.readyweapon != WeaponType::SuperShotgun
            {
                new_weapon = WeaponType::SuperShotgun;
            }

            if self.status.weaponowned[new_weapon as usize] && new_weapon != self.status.readyweapon
            {
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
            if !self.status.usedown {
                self.status.usedown = true;
                if let Some(mobj) = self.mobj_mut() {
                    mobj.use_lines();
                }
            }
        } else {
            self.status.usedown = false;
        }

        self.move_player_sprites();

        // Powers and timers
        if self.status.powers[PowerType::Strength as usize] != 0 {
            // Strength counts up to diminish fade.
            self.status.powers[PowerType::Strength as usize] += 1;
        }

        if self.status.powers[PowerType::Invulnerability as usize] != 0 {
            self.status.powers[PowerType::Invulnerability as usize] -= 1;
        }

        if self.status.powers[PowerType::Infrared as usize] != 0 {
            self.status.powers[PowerType::Infrared as usize] -= 1;
        }

        if self.status.powers[PowerType::IronFeet as usize] != 0 {
            self.status.powers[PowerType::IronFeet as usize] -= 1;
        }

        if self.status.powers[PowerType::Invisibility as usize] != 0 {
            self.status.powers[PowerType::Invisibility as usize] -= 1;
            if self.status.powers[PowerType::Invisibility as usize] == 0 {
                if let Some(mobj) = self.mobj_mut() {
                    mobj.flags.remove(MapObjFlag::Shadow);
                }
            }
        }

        // Screen flashing, red, damage etc
        if self.status.damagecount != 0 {
            self.status.damagecount -= 1;
        }

        if self.status.bonuscount != 0 {
            self.status.bonuscount -= 1;
        }

        if self.status.attacked_angle_count != 0 {
            self.status.attacked_angle_count -= 1;
        }

        // Setting the colourmaps
        let invulnerability = self.status.powers[PowerType::Invulnerability as usize];
        let infrared = self.status.powers[PowerType::Infrared as usize];
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
    pub fn death_think(&mut self, level: &mut LevelState) {
        self.move_player_sprites();

        if let Some(mobj) = self.mobj {
            let mobj = unsafe { &mut *mobj };
            if self.viewheight >= 6 {
                self.viewheight -= FixedT::ONE;
            }
            if self.viewheight == 6 {
                info!("You died! Press use-button to respawn");
            }

            self.onground = mobj.z <= mobj.floorz;
            self.calculate_height(level.level_time);

            if let Some(attacker) = self.attacker {
                let attacker = unsafe { &mut *attacker };
                if !std::ptr::eq(mobj, attacker) {
                    // OG: an = R_PointToAngle2(mo->x, mo->y, attacker->x, attacker->y)
                    let dx = attacker.x - mobj.x;
                    let dy = attacker.y - mobj.y;
                    let an = r_point_to_angle(dx, dy);
                    // OG: delta = an - mo->angle (unsigned BAM subtraction)
                    let delta = an.wrapping_sub(mobj.angle.to_bam());

                    if delta < ANG5_BAM || delta > (0u32).wrapping_sub(ANG5_BAM) {
                        // Within ±5 degrees: snap to target angle
                        mobj.angle = Angle::from_bam(an);
                        if self.status.damagecount > 0 {
                            self.status.damagecount -= 1;
                        }
                    } else if delta < ANG180 {
                        mobj.angle = Angle::from_bam(mobj.angle.to_bam().wrapping_add(ANG5_BAM));
                    } else {
                        mobj.angle = Angle::from_bam(mobj.angle.to_bam().wrapping_sub(ANG5_BAM));
                    }
                }
            } else if self.status.damagecount > 0 {
                self.status.damagecount -= 1;
            }
        }

        if self.cmd.buttons & TIC_CMD_BUTTONS.bt_use != 0 {
            self.player_state = PlayerState::Reborn;
        }
    }
}
