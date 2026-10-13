//! Generalised sector effects.
//!
//! One mover thinker covers every vanilla door, floor, ceiling and platform;
//! one light thinker covers every vanilla light pattern. Behaviour is selected
//! by policy fields set at construction. This module owns the effect data,
//! think functions, and thinker spawning; the vanilla parameterisations live
//! in [`mover_presets`] and [`light_presets`].

pub(crate) mod light_presets;
pub(crate) mod mover_presets;

use std::ptr::{self, null_mut};

use sound_common::SfxName;

use crate::SectorExt as _;
use crate::env::specials::{PlaneResult, move_plane};
use crate::env::switch::start_sector_sound;
use crate::level::LevelState;
use crate::thinker::{Think, Thinker, ThinkerAlloc, ThinkerData};
use level::byte_enum;
use level::env_query::find_min_light_surrounding;
use level::map_defs::{Sector, SectorHeight};
use level::special_encode::SectorSpecial;
use level::{MapPtr, MovementType};
use math::p_random;

/// Light change per tic for [`LightPattern::Glow`].
const GLOWSPEED: usize = 8;
/// Tics between [`LightPattern::Fire`] updates.
const FIRE_TICS: i32 = 4;
/// Light quantum for [`LightPattern::Fire`].
const FIRE_STEP: usize = 16;

byte_enum! {
    /// Which surface a [`MoveEffect`] drives. Discriminants are the
    /// `floor_or_ceiling` selector [`move_plane`] takes.
    pub enum MoverPlane {
        Floor = 0,
        Ceiling = 1,
    }
}

byte_enum! {
    /// What a [`MoveEffect`] is doing right now.
    pub enum MoverPhase {
        Up = 0,
        Down = 1,
        Wait = 2,
        InStasis = 3,
    }
}

byte_enum! {
    /// Response to crushing something while moving in the crushing direction.
    pub enum CrushResponse {
        /// Keep going (floors, close-only doors).
        Ignore = 0,
        /// Turn around (doors bounce open, lifts drop back).
        Reverse = 1,
        /// Drop to an eighth speed and grind on (crushers).
        SlowGrind = 2,
    }
}

byte_enum! {
    /// What a [`MoveEffect`] does on reaching one end of its travel.
    pub enum ArrivalAction {
        /// Apply any change, detach from the sector, remove the thinker.
        Finish = 0,
        /// Hold for `wait` tics, then travel the other way.
        WaitThenReverse = 1,
        /// Travel the other way immediately (crushers).
        Reverse = 2,
    }
}

byte_enum! {
    /// Which light animation a [`LightEffect`] runs.
    pub enum LightPattern {
        /// Random on/off with randomised hold times.
        Flash = 0,
        /// Regular on/off with fixed hold times.
        Strobe = 1,
        /// Random flicker within a band, 4-tic steps.
        Fire = 2,
        /// Smooth ramp between min and max.
        Glow = 3,
    }
}

/// The lower end of a [`MoveEffect`]'s travel. Doors chase the sector floor as
/// it moves rather than a height fixed when the door was spawned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MoveTarget {
    Fixed(SectorHeight),
    LiveFloor,
}

/// Sector floor texture and special applied when a [`MoveEffect`] finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoverChange {
    pub newspecial: SectorSpecial,
    pub texture: usize,
}

/// Sounds a [`MoveEffect`] emits, keyed by the transition that triggers them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MoverSounds {
    /// Entering [`MoverPhase::Up`] from a wait or a reversal.
    pub start_up: Option<SfxName>,
    /// Entering [`MoverPhase::Down`] from a wait or a reversal.
    pub start_down: Option<SfxName>,
    /// Entering [`MoverPhase::Wait`].
    pub start_wait: Option<SfxName>,
    /// Reaching an end that finishes the mover.
    pub finish: Option<SfxName>,
    /// Every 8 tics while travelling.
    pub moving: Option<SfxName>,
}

/// A sector surface in motion: doors, floors, ceilings, crushers and lifts.
pub struct MoveEffect {
    pub thinker: *mut Thinker,
    pub sector: MapPtr<Sector>,
    /// Sector tag a stop/start special matches on; see `stoppable`.
    pub tag: i16,
    pub plane: MoverPlane,
    pub speed: SectorHeight,
    /// Speed restored on reaching the bottom, undoing [`CrushResponse::SlowGrind`].
    pub base_speed: SectorHeight,
    pub low: MoveTarget,
    /// Upper end of travel, and the destination of a one-shot rising floor.
    pub high: SectorHeight,
    pub phase: MoverPhase,
    /// Phase to resume when released from [`MoverPhase::InStasis`].
    pub stasis_phase: MoverPhase,
    /// Whether a tagged stop/start special can put this mover into stasis.
    /// Only lifts are, matching vanilla's active-platform list.
    pub stoppable: bool,
    /// Tics to hold in [`MoverPhase::Wait`].
    pub wait: i32,
    /// Tics left of the current hold.
    pub countdown: i32,
    pub crush: bool,
    pub crush_response: CrushResponse,
    pub at_top: ArrivalAction,
    pub at_bottom: ArrivalAction,
    pub change: Option<MoverChange>,
    pub sounds: MoverSounds,
}

impl MoveEffect {
    /// Height of the surface this mover drives.
    pub fn plane_height(&self) -> SectorHeight {
        match self.plane {
            MoverPlane::Floor => self.sector.floorheight,
            MoverPlane::Ceiling => self.sector.ceilingheight,
        }
    }

    /// Lower end of travel, resolved now.
    pub fn low_height(&self) -> SectorHeight {
        match self.low {
            MoveTarget::Fixed(h) => h,
            MoveTarget::LiveFloor => self.sector.floorheight,
        }
    }

    /// True when travelling the way that can crush: ceilings down, floors up.
    fn is_crushing_dir(&self, up: bool) -> bool {
        match self.plane {
            MoverPlane::Floor => up,
            MoverPlane::Ceiling => !up,
        }
    }

    fn play(&self, sfx: Option<SfxName>, level: &LevelState) {
        if let Some(sfx) = sfx {
            start_sector_sound(self.sector.lines[0].as_ref(), sfx, &level.snd_command);
        }
    }

    fn enter_up(&mut self, level: &LevelState) {
        self.phase = MoverPhase::Up;
        self.play(self.sounds.start_up, level);
    }

    fn enter_down(&mut self, level: &LevelState) {
        self.phase = MoverPhase::Down;
        self.play(self.sounds.start_down, level);
    }

    fn enter_wait(&mut self, level: &LevelState) {
        self.countdown = self.wait;
        self.phase = MoverPhase::Wait;
        self.play(self.sounds.start_wait, level);
    }
}

/// Skeleton mover: still, no crush, finishing at both ends.
fn base_mover(sector: MapPtr<Sector>, plane: MoverPlane, speed: SectorHeight) -> MoveEffect {
    MoveEffect {
        thinker: null_mut(),
        tag: sector.tag,
        sector,
        plane,
        speed,
        base_speed: speed,
        low: MoveTarget::Fixed(SectorHeight::ZERO),
        high: SectorHeight::ZERO,
        phase: MoverPhase::Up,
        stasis_phase: MoverPhase::Up,
        stoppable: false,
        wait: 0,
        countdown: 0,
        crush: false,
        crush_response: CrushResponse::Ignore,
        at_top: ArrivalAction::Finish,
        at_bottom: ArrivalAction::Finish,
        change: None,
        sounds: MoverSounds::default(),
    }
}

/// Insert a built mover, attach it to its sector, and hand back a stable
/// pointer for the active-mover list.
fn push_mover(mover: MoveEffect, thinkers: &mut ThinkerAlloc) -> Option<*mut MoveEffect> {
    let mut sector = mover.sector.clone();
    let thinker = MoveEffect::create_thinker(ThinkerData::Move(mover), MoveEffect::think);
    let ptr = thinkers.push::<MoveEffect>(thinker)?;
    ptr.set_obj_thinker_ptr();
    sector.set_sector_mover(ptr);
    Some(ptr::from_mut(ptr.mover_mut()))
}

impl Think for MoveEffect {
    fn think(object: &mut Thinker, level: &mut LevelState) -> bool {
        let mover = object.mover_mut();
        #[cfg(feature = "null_check")]
        assert!(!mover.thinker.is_null(), "mover thinker was null");

        match mover.phase {
            MoverPhase::InStasis => {}
            MoverPhase::Wait => {
                mover.countdown -= 1;
                if mover.countdown == 0 {
                    if mover.plane_height() == mover.low_height() {
                        mover.enter_up(level);
                    } else {
                        mover.enter_down(level);
                    }
                }
            }
            MoverPhase::Up | MoverPhase::Down => {
                let up = matches!(mover.phase, MoverPhase::Up);
                let crushing_dir = mover.is_crushing_dir(up);

                if level.level_time & 7 == 0 {
                    mover.play(mover.sounds.moving, level);
                }

                let dest = if up { mover.high } else { mover.low_height() };
                let res = move_plane(
                    mover.sector.clone(),
                    mover.speed,
                    dest,
                    crushing_dir && mover.crush,
                    mover.plane as i32,
                    if up { 1 } else { -1 },
                    level,
                );

                match res {
                    PlaneResult::PastDest => {
                        let action = if up { mover.at_top } else { mover.at_bottom };
                        match action {
                            ArrivalAction::Finish => {
                                mover.play(mover.sounds.finish, level);
                                finish(mover, level);
                            }
                            ArrivalAction::WaitThenReverse => mover.enter_wait(level),
                            ArrivalAction::Reverse => {
                                if up {
                                    mover.enter_down(level);
                                } else {
                                    mover.speed = mover.base_speed;
                                    mover.enter_up(level);
                                }
                            }
                        }
                    }
                    PlaneResult::Crushed if crushing_dir => match mover.crush_response {
                        CrushResponse::Ignore => {}
                        CrushResponse::Reverse => {
                            if !mover.crush {
                                if up {
                                    mover.enter_down(level);
                                } else {
                                    mover.enter_up(level);
                                }
                            }
                        }
                        CrushResponse::SlowGrind => mover.speed = mover.base_speed / 8,
                    },
                    _ => {}
                }
            }
        }

        true
    }

    fn set_thinker_ptr(&mut self, ptr: *mut Thinker) {
        self.thinker = ptr;
    }

    fn thinker_mut(&mut self) -> &mut Thinker {
        #[cfg(feature = "null_check")]
        assert!(!self.thinker.is_null(), "mover thinker was null");
        // SAFETY: set by `set_thinker_ptr` on insert; the arena outlives the mover.
        unsafe { Thinker::from_erased(self.thinker) }
    }
}

/// Apply the arrival change, detach from the sector, and drop the thinker.
fn finish(mover: &mut MoveEffect, level: &mut LevelState) {
    if let Some(change) = mover.change {
        mover.sector.special = change.newspecial;
        mover.sector.floorpic = change.texture;
        level
            .level_data
            .bsp_3d
            .update_flat_texture(mover.sector.num as usize, MovementType::Floor);
    }
    mover.sector.specialdata = None;
    // SAFETY: the mover is live here; it is only marked for removal by this call.
    unsafe { level.remove_active_mover(mover) };
}

/// An animated sector light level.
pub struct LightEffect {
    pub thinker: *mut Thinker,
    pub sector: MapPtr<Sector>,
    pub pattern: LightPattern,
    pub min_light: usize,
    pub max_light: usize,
    /// Tics left of the current hold.
    pub countdown: i32,
    /// Dark hold: tics for [`LightPattern::Strobe`], random mask for
    /// [`LightPattern::Flash`].
    pub dark_time: i32,
    /// Bright hold: tics for [`LightPattern::Strobe`], random mask for
    /// [`LightPattern::Flash`].
    pub bright_time: i32,
    pub direction: i32,
}

/// Insert a built light into the thinker arena.
fn push_light(light: LightEffect, level: &mut LevelState) {
    let thinker = LightEffect::create_thinker(ThinkerData::Light(light), LightEffect::think);
    if let Some(ptr) = level.thinkers.push::<LightEffect>(thinker) {
        ptr.set_obj_thinker_ptr();
    }
}

/// Skeleton light, ramp fields idle. Clears the sector special, as every
/// vanilla light spawner does.
fn base_light(sector: &mut Sector, pattern: LightPattern) -> LightEffect {
    sector.special = SectorSpecial::default();
    LightEffect {
        thinker: null_mut(),
        pattern,
        min_light: find_min_light_surrounding(MapPtr::new(sector), sector.lightlevel),
        max_light: sector.lightlevel,
        sector: MapPtr::new(sector),
        countdown: 0,
        dark_time: 0,
        bright_time: 0,
        direction: 0,
    }
}

impl Think for LightEffect {
    fn think(object: &mut Thinker, _level: &mut LevelState) -> bool {
        let light = object.light_mut();
        #[cfg(feature = "null_check")]
        assert!(!light.thinker.is_null(), "light thinker was null");

        if matches!(light.pattern, LightPattern::Glow) {
            match light.direction {
                -1 => {
                    if light.sector.lightlevel >= GLOWSPEED {
                        light.sector.lightlevel -= GLOWSPEED;
                    }
                    if light.sector.lightlevel <= light.min_light {
                        light.sector.lightlevel += GLOWSPEED;
                        light.direction = 1;
                    }
                }
                1 => {
                    light.sector.lightlevel += GLOWSPEED;
                    if light.sector.lightlevel >= light.max_light {
                        if light.sector.lightlevel >= GLOWSPEED {
                            light.sector.lightlevel -= GLOWSPEED;
                        }
                        light.direction = -1;
                    }
                }
                _ => {}
            }
            return false;
        }

        light.countdown -= 1;
        if light.countdown != 0 {
            return false;
        }

        match light.pattern {
            LightPattern::Flash => {
                if light.sector.lightlevel == light.max_light {
                    light.sector.lightlevel = light.min_light;
                    light.countdown = (p_random() & light.dark_time) + 1;
                } else {
                    light.sector.lightlevel = light.max_light;
                    light.countdown = (p_random() & light.bright_time) + 1;
                }
            }
            LightPattern::Strobe => {
                if light.sector.lightlevel == light.min_light {
                    light.sector.lightlevel = light.max_light;
                    light.countdown = light.bright_time;
                } else {
                    light.sector.lightlevel = light.min_light;
                    light.countdown = light.dark_time;
                }
            }
            LightPattern::Fire => {
                let amount = ((p_random() & 3) as usize) * FIRE_STEP;
                if light.sector.lightlevel >= amount {
                    light.sector.lightlevel = light.max_light - amount;
                }
                if light.sector.lightlevel < light.min_light {
                    light.sector.lightlevel = light.min_light;
                }
                light.countdown = FIRE_TICS;
            }
            LightPattern::Glow => {}
        }

        false
    }

    fn set_thinker_ptr(&mut self, ptr: *mut Thinker) {
        self.thinker = ptr;
    }

    fn thinker_mut(&mut self) -> &mut Thinker {
        #[cfg(feature = "null_check")]
        assert!(!self.thinker.is_null(), "light thinker was null");
        // SAFETY: set by `set_thinker_ptr` on insert; the arena outlives the light.
        unsafe { Thinker::from_erased(self.thinker) }
    }
}
