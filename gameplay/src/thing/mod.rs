//! The bulk of map entities, typically shown as sprites. Things like monsters,
//! giblets, rockets and plasma shots etc, items.
//!
//! `MapObject` is also used for moving doors, platforms, floors, and ceilings.

mod interact;
pub use interact::*;
mod movement;
pub use movement::*;
use sound_common::SfxName;
pub(crate) mod enemy;
mod shooting;

use bitflags::bitflags;
use std::fmt::Debug;
use std::ptr::null_mut;

use self::movement::SubSectorMinMax;

use crate::SectorExt;
use crate::doom_def::{MELEERANGE, MTF_SINGLE_PLAYER};
use crate::level::LevelState;
use crate::thinker::{Think, Thinker, ThinkerData};
use game_config::Skill;
use level::MapPtr;
use log::{debug, error, trace, warn};
use math::{Bam, FixedT, p_aprox_distance, r_point_to_angle};
use wad::types::WadThing;

use crate::bsp_trace::BestSlide;
use crate::doom_def::{MAXPLAYERS, MTF_AMBUSH, ONCEILINGZ, ONFLOORZ, TICRATE, VIEWHEIGHT};
use crate::info::{MOBJINFO, MapObjInfo, MapObjKind, STATES, SpriteNum, StateData, StateNum};
use crate::player::{Player, PlayerState};
use level::map_defs::SubSector;
use math::{ANG45, Angle, p_random, p_subrandom};

/// OG Doom MAPBLOCKSHIFT = FRACBITS + 7 = 23
const MAPBLOCKSHIFT: i32 = 23;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct MapObjFlag: u32 {
        /// Call P_SpecialThing when touched.
        const Special = 1;
        /// Blocks.
        const Solid = 2;
        /// Can be hit.
        const Shootable = 4;
        /// Don't use the sector links (invisible but touchable).
        const Nosector = 8;
        /// Don't use the block links (inert but displayable)
        const Noblockmap = 16;
        /// Not to be activated by sound, deaf monster.
        const Ambush = 32;
        /// Will try to attack right back.
        const Justhit = 64;
        /// Will take at least one step before attacking.
        const Justattacked = 128;
        /// On level spawning (initial position), hang from ceiling instead of stand on floor.
        const Spawnceiling = 256;
        /// Don't apply gravity (every tic), that is, object will float, keeping current height.
        const Nogravity = 512;
        /// This allows jumps from high places.
        const Dropoff = 0x400;
        /// For players, will pick up items.
        const Pickup = 0x800;
        /// Player cheat. ???
        const Noclip = 0x1000;
        /// Player: keep info about sliding along walls.
        const Slide = 0x2000;
        /// Allow moves to any height, no gravity. For active floaters, e.g. cacodemons.
        const Float = 0x4000;
        /// Don't cross lines ??? or look at heights on teleport.
        const Teleport = 0x8000;
        /// Don't hit same species, explode on block.
        const Missile = 0x10000;
        /// Dropped by a demon, not level spawned.
        const Dropped = 0x20000;
        /// Use fuzzy draw (shadow demons or spectres), temporary player invisibility powerup.
        const Shadow = 0x40000;
        /// Don't bleed when shot (use puff).
        const Noblood = 0x80000;
        /// Don't stop moving halfway off a step, have dead bodies slide down all the way.
        const Corpse = 0x100000;
        /// Floating to a height for a move, don't auto float to target's height.
        const Infloat = 0x200000;
        /// On kill, count this enemy towards intermission kill total.
        const Countkill = 0x400000;
        /// On picking up, count this item towards intermission item total.
        const Countitem = 0x800000;
        /// Special handling: skull in flight.
        const Skullfly = 0x1000000;
        /// Don't spawn this object in death match mode.
        const Notdmatch = 0x2000000;
        /// Player color translation table bits (bits 26-27).
        const Translation = 0xC000000;
    }
}

/// Bit shift for the Translation color field within MapObjFlag.
pub const TRANSSHIFT: u32 = 26;

pub struct MapObject {
    /// `MapObject` is owned by the `Thinker`. If the `MapObject` is ever moved
    /// out of the `Thinker` then you must update sector thing lists and self
    /// linked list. This is a pointer to the `Thinker` storage.
    pub(crate) thinker: *mut Thinker,
    /// Specific to Doom II. These are pointers to targets that the
    /// final boss shoots demon spawn cubes towards. It is expected that
    /// because these are level items they will never shift their memory
    /// location. The raw pointers here are to map entities that never
    /// move/delete themselves.
    pub(crate) boss_targets: Vec<*mut Thinker>,
    /// Specific to Doom II. The current target (spawn point for demons)
    pub(crate) boss_target_on: usize,
    /// Info for drawing: position.
    pub x: FixedT,
    pub y: FixedT,
    pub z: FixedT,
    /// Previous tic position for rendering interpolation.
    pub prev_x: FixedT,
    pub prev_y: FixedT,
    pub prev_z: FixedT,
    // More drawing info: to determine current sprite.
    /// orientation
    pub angle: Angle<Bam>,
    /// used to find patch_t and flip value
    pub sprite: SpriteNum,
    /// might be ORed with FF_FULLBRIGHT
    pub frame: u32,
    /// Link to the next `Thinker` in this sector. You can think of this as a
    /// separate Linked List to the `Thinker` linked list used in storage. I
    /// does mean that you will need to unlink an object both here, and in the
    /// Thinker storage if removing one.
    pub(crate) s_next: Option<*mut Thinker>,
    /// Link to the previous `Thinker` in this sector
    pub(crate) s_prev: Option<*mut Thinker>,
    /// Link to next `MapObject` in the same blockmap cell
    pub(crate) b_next: Option<*mut MapObject>,
    /// Link to previous `MapObject` in the same blockmap cell
    pub(crate) b_prev: Option<*mut MapObject>,
    /// The subsector this object is currently in. When a map object is spawned
    /// `set_thing_position()` is called which then sets this to a valid
    /// subsector, making this safe in 99% of cases.
    pub subsector: MapPtr<SubSector>,
    /// The closest interval over all contacted Sectors.
    pub(crate) floorz: FixedT,
    pub(crate) ceilingz: FixedT,
    /// For movement checking.
    pub(crate) radius: FixedT,
    pub(crate) height: FixedT,
    /// Momentum, used to update position.
    pub(crate) momx: FixedT,
    pub(crate) momy: FixedT,
    pub(crate) momz: FixedT,
    /// If == validcount, already checked.
    pub(crate) valid_count: usize,
    /// The type of object
    pub(crate) kind: MapObjKind,
    /// &mobjinfo[thing.type]
    pub(crate) info: MapObjInfo,
    pub(crate) tics: i32,
    /// state tic counter
    // TODO: probably only needs to be an index to the array
    //  using the enum as the indexer
    pub state: &'static StateData,
    pub flags: MapObjFlag,
    pub health: i32,
    /// Movement direction, movement generation (zig-zagging).
    /// 0-7
    pub(crate) movedir: MoveDir,
    /// when 0, select a new dir
    pub(crate) movecount: i32,
    /// The best slide move for a player object
    pub(crate) best_slide: BestSlide,
    /// Thing being chased/attacked (or NULL),
    /// also the originator for missiles.
    pub(crate) target: Option<*mut Thinker>,
    pub(crate) tracer: Option<*mut Thinker>,
    /// Reaction time: if non 0, don't attack yet.
    /// Used by player to freeze a bit after teleporting.
    pub(crate) reactiontime: i32,
    /// If >0, the target will be chased
    /// no matter what (even if shot)
    pub(crate) threshold: i32,
    /// Additional info record for player avatars only. Only valid if type ==
    /// MT_PLAYER. RUST: If this is not `None` then the pointer is
    /// guaranteed to point to a player
    pub(crate) player: Option<*mut Player>,
    /// Player number last looked for, 1-4 (does not start at 0)
    pub(crate) lastlook: usize,
    /// For nightmare respawn.
    pub(crate) spawnpoint: WadThing,
    // Thing being chased/attacked for tracers.
    // struct mobj_s*	tracer;
    /// Every map object needs a link to the level structure to read various
    /// level elements and possibly change some (sector links for example).
    pub(crate) level: *mut LevelState,
}

impl MapObject {
    /// Construct a `MapObject` from save data. Cross-references (target,
    /// tracer, player) are left as `None`/null; the caller patches them.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_save_data(
        x: f32,
        y: f32,
        z: f32,
        angle: Angle<Bam>,
        sprite: SpriteNum,
        frame: u32,
        floorz: f32,
        ceilingz: f32,
        radius: f32,
        height: f32,
        momx: f32,
        momy: f32,
        momz: f32,
        kind: MapObjKind,
        info: MapObjInfo,
        tics: i32,
        state: &'static crate::info::StateData,
        flags: MapObjFlag,
        health: i32,
        movedir: MoveDir,
        movecount: i32,
        reactiontime: i32,
        threshold: i32,
        lastlook: usize,
        spawnpoint: WadThing,
        level: *mut LevelState,
    ) -> Self {
        Self {
            thinker: null_mut(),
            boss_targets: Vec::new(),
            boss_target_on: 0,
            x: FixedT::from_f32(x),
            y: FixedT::from_f32(y),
            z: FixedT::from_f32(z),
            prev_x: FixedT::from_f32(x),
            prev_y: FixedT::from_f32(y),
            prev_z: FixedT::from_f32(z),
            angle,
            sprite,
            frame,
            s_next: None,
            s_prev: None,
            b_next: None,
            b_prev: None,
            subsector: unsafe { MapPtr::new_null() },
            floorz: FixedT::from_f32(floorz),
            ceilingz: FixedT::from_f32(ceilingz),
            radius: FixedT::from_f32(radius),
            height: FixedT::from_f32(height),
            momx: FixedT::from_f32(momx),
            momy: FixedT::from_f32(momy),
            momz: FixedT::from_f32(momz),
            valid_count: 0,
            kind,
            info,
            tics,
            state,
            flags,
            health,
            movedir,
            movecount,
            best_slide: BestSlide::default(),
            reactiontime,
            threshold,
            target: None,
            tracer: None,
            player: None,
            lastlook,
            spawnpoint,
            level,
        }
    }
}

impl Debug for MapObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapObject")
            .field("x", &self.x)
            .field("y", &self.y)
            .field("z", &self.z)
            .field("angle", &self.angle)
            .field("sprite", &self.sprite)
            .field("frame", &self.frame)
            .field("floorz", &self.floorz)
            .field("ceilingz", &self.ceilingz)
            .field("radius", &self.radius)
            .field("height", &self.height)
            .field("momx", &self.momx)
            .field("momy", &self.momy)
            .field("momz", &self.momz)
            .field("valid_count", &self.valid_count)
            .field("kind", &self.kind)
            .field("info", &self.info)
            .field("tics", &self.tics)
            .field("state", &self.state)
            .field("flags", &self.flags)
            .field("health", &self.health)
            .field("movecount", &self.movecount)
            .field("reactiontime", &self.reactiontime)
            .field("threshold", &self.threshold)
            .finish_non_exhaustive()
    }
}

impl MapObject {
    #[allow(clippy::too_many_arguments)]
    fn new(
        x: FixedT,
        y: FixedT,
        z: FixedT,
        reactiontime: i32,
        kind: MapObjKind,
        info: MapObjInfo,
        state: &'static StateData,
        level: *mut LevelState,
    ) -> Self {
        Self {
            thinker: null_mut(),
            boss_targets: Vec::new(),
            boss_target_on: 0,
            player: None,
            x,
            y,
            z,
            prev_x: x,
            prev_y: y,
            prev_z: z,
            angle: Angle::new(0.0),
            sprite: state.sprite,
            frame: state.frame,
            floorz: FixedT::ZERO,
            ceilingz: FixedT::ZERO,
            radius: FixedT::from_f32(info.radius),
            height: FixedT::from_f32(info.height),
            momx: FixedT::ZERO,
            momy: FixedT::ZERO,
            momz: FixedT::ZERO,
            valid_count: 0,
            flags: info.flags,
            health: info.spawnhealth,
            tics: state.tics,
            movedir: MoveDir::East,
            movecount: 0,
            best_slide: BestSlide::default(),
            reactiontime,
            threshold: 0,
            lastlook: p_random() as usize % MAXPLAYERS as usize,
            spawnpoint: WadThing::default(),
            target: None,
            tracer: None,
            s_next: None,
            s_prev: None,
            b_next: None,
            b_prev: None,
            subsector: unsafe { MapPtr::new_null() },
            state,
            info,
            kind,
            level,
        }
    }

    /// Momentum X (read-only, for debug/comparison).
    pub fn momx(&self) -> FixedT {
        self.momx
    }

    /// Momentum Y (read-only, for debug/comparison).
    pub fn momy(&self) -> FixedT {
        self.momy
    }

    /// Momentum Z (read-only, for debug/comparison).
    pub fn momz(&self) -> FixedT {
        self.momz
    }

    /// Floor Z (read-only, for debug/comparison).
    pub fn floorz(&self) -> FixedT {
        self.floorz
    }

    /// Ceiling Z (read-only, for debug/comparison).
    pub fn ceilingz(&self) -> FixedT {
        self.ceilingz
    }

    /// State table index (for trace comparison with OG Doom).
    pub fn state_index(&self) -> usize {
        (self.state as *const _ as usize - STATES.as_ptr() as usize)
            / std::mem::size_of::<StateData>()
    }

    /// State tics remaining.
    pub fn tics(&self) -> i32 {
        self.tics
    }

    /// Object type enum.
    pub fn kind(&self) -> MapObjKind {
        self.kind
    }

    /// Movement direction (0-7 cardinal, 8=none).
    pub fn movedir(&self) -> u32 {
        self.movedir as u32
    }

    /// Move countdown.
    pub fn movecount(&self) -> i32 {
        self.movecount
    }

    /// Reaction time counter.
    pub fn reactiontime(&self) -> i32 {
        self.reactiontime
    }

    /// Chase threshold.
    pub fn threshold(&self) -> i32 {
        self.threshold
    }

    /// Whether this mobj has a target.
    pub fn has_target(&self) -> bool {
        self.target.is_some()
    }

    pub(crate) fn level(&self) -> &LevelState {
        #[cfg(feature = "null_check")]
        if self.level.is_null() {
            std::panic!("MapObject level pointer was null");
        }
        unsafe { &*self.level }
    }

    pub(crate) fn level_mut(&mut self) -> &mut LevelState {
        #[cfg(feature = "null_check")]
        if self.level.is_null() {
            std::panic!("MapObject level pointer was null");
        }
        unsafe { &mut *self.level }
    }

    pub fn player(&self) -> Option<&Player> {
        self.player.map(|p| unsafe {
            #[cfg(feature = "null_check")]
            if p.is_null() {
                std::panic!("MapObject player pointer was null");
            }
            &*p
        })
    }

    pub(crate) fn player_mut(&mut self) -> Option<&mut Player> {
        self.player.map(|p| unsafe {
            #[cfg(feature = "null_check")]
            if p.is_null() {
                std::panic!("MapObject player pointer was null");
            }
            &mut *p
        })
    }

    pub(crate) fn target(&self) -> Option<&MapObject> {
        self.target.map(|t| unsafe {
            #[cfg(feature = "null_check")]
            if t.is_null() {
                std::panic!("MapObject target pointer was null");
            }
            (*t).mobj()
        })
    }

    pub(crate) fn target_mut(&mut self) -> Option<&mut MapObject> {
        self.target.map(|t| unsafe {
            #[cfg(feature = "null_check")]
            if t.is_null() {
                std::panic!("MapObject target pointer was null");
            }
            (*t).mobj_mut()
        })
    }

    /// P_SpawnPlayer
    /// Called when a player is spawned on the level.
    /// Most of the player structure stays unchanged
    ///  between levels.
    ///
    /// Called in game-exe.c
    fn p_spawn_player(
        mthing: &WadThing,
        level: &mut LevelState,
        players: &mut [Player],
        active_players: &[bool; MAXPLAYERS],
    ) {
        trace!("Player spawn check");
        if mthing.kind == 0 {
            return;
        }

        // not playing?
        if !active_players[(mthing.kind - 1) as usize] {
            return;
        }

        // TODO: Properly sort this out
        let player = &mut players[0];
        trace!("Spawing player 1");

        if player.player_state == PlayerState::Reborn {
            player.reborn();
        }

        // Doom spawns this in it's memory manager then passes a pointer back. As fasr
        // as I can see the Player object owns this.
        let mobj = MapObject::spawn_map_object(
            (mthing.x as i32).into(),
            (mthing.y as i32).into(),
            ONFLOORZ.into(),
            MapObjKind::MT_PLAYER,
            level,
        );

        // set color translations for player sprites

        let mobj_ptr_mut = unsafe { &mut *mobj };
        if mthing.kind > 1 {
            mobj_ptr_mut.flags |=
                MapObjFlag::from_bits_truncate((mthing.kind as u32 - 1) << TRANSSHIFT);
        }

        // TODO: check this angle stuff
        mobj_ptr_mut.angle = Angle::from_bam(ANG45.wrapping_mul((mthing.angle as u32) / 45));
        mobj_ptr_mut.health = player.status.health;
        mobj_ptr_mut.player = Some(player);

        player.set_mobj(mobj);
        player.player_state = PlayerState::Live;
        player.refire = 0;
        player.message = None;
        player.status.damagecount = 0;
        player.status.bonuscount = 0;
        player.extralight = 0;
        player.fixedcolormap = 0;
        player.viewheight = FixedT::from(VIEWHEIGHT);

        // Sync prev_render so first frame doesn't lerp from origin
        player.save_prev_render();

        // // setup gun psprite
        // TODO: P_SetupPsprites(p);

        // // give all cards in death match mode
        // if deathmatch {
        //     for i in 0..Card::NUMCARDS as usize {
        //         p.cards[i] = true;
        //     }
        // }

        // if mthing.kind - 1 == consoleplayer {
        //     // wake up the status bar
        // TODO:  ST_Start();
        //     // wake up the heads up text
        // TODO:  HU_Start();
        // }
    }

    /// P_SpawnMapThing
    pub fn p_spawn_map_thing(
        mthing: WadThing,
        no_monsters: bool,
        level: &mut LevelState,
        players: &mut [Player],
        active_players: &[bool; MAXPLAYERS],
    ) {
        // count deathmatch start positions
        if mthing.kind == 11 {
            if level.deathmatch_p.len() < level.deathmatch_starts.len() {
                level.deathmatch_p.push(mthing);
            }
            return;
        }

        // check for players specially
        if mthing.kind <= 4 && mthing.kind != 0 {
            // save spots for respawning in network games
            level.player_starts[(mthing.kind - 1) as usize] = Some(mthing);
            if level.options.deathmatch == 0 {
                MapObject::p_spawn_player(&mthing, level, players, active_players);
            }
            return;
        }

        // check for appropriate skill level
        if level.options.deathmatch == 0 && mthing.flags & MTF_SINGLE_PLAYER != 0 {
            return;
        }
        let bit: i16;
        if level.options.skill == Skill::Baby {
            bit = 1;
        } else if level.options.skill == Skill::Nightmare {
            bit = 4;
        } else {
            bit = 1 << (level.options.skill as i16 - 1);
        }

        if mthing.flags & bit == 0 {
            return;
        }

        // find which type to spawn
        let mut i = 0;
        for n in 0..MapObjKind::Count as u16 {
            if mthing.kind == MOBJINFO[n as usize].doomednum as i16 {
                i = n;
                break;
            }
        }

        if i == MapObjKind::Count as u16 {
            error!(
                "P_SpawnMapThing: Unknown type {} at ({}, {})",
                mthing.kind, mthing.x, mthing.y
            );
        }

        // don't spawn keycards and players in deathmatch
        if level.options.deathmatch != 0
            && MOBJINFO[i as usize].flags.contains(MapObjFlag::Notdmatch)
        {
            return;
        }

        // TODO: don't spawn any monsters if -nomonsters
        let kind = MapObjKind::from(i);
        if no_monsters
            && (kind == MapObjKind::MT_SKULL
                || MOBJINFO[i as usize].flags.contains(MapObjFlag::Countkill))
        {
            return;
        }

        let x = mthing.x as i32;
        let y = mthing.y as i32;
        let z = if MOBJINFO[i as usize]
            .flags
            .contains(MapObjFlag::Spawnceiling)
        {
            ONCEILINGZ
        } else {
            ONFLOORZ
        };

        let mobj =
            MapObject::spawn_map_object(x.into(), y.into(), z.into(), MapObjKind::from(i), level);
        let mobj = unsafe { &mut *mobj };
        if mobj.tics > 0 {
            mobj.tics = 1 + (p_random() % mobj.tics);
        }
        if mobj.flags.contains(MapObjFlag::Countkill) {
            level.total_level_kills += 1;
        }
        if mobj.flags.contains(MapObjFlag::Countitem) {
            level.total_level_items += 1;
        }

        // TODO: check the angle is correct
        mobj.angle = Angle::from_bam(ANG45.wrapping_mul((mthing.angle as u32) / 45));
        if mthing.flags & MTF_AMBUSH != 0 {
            mobj.flags.insert(MapObjFlag::Ambush);
        }

        mobj.spawnpoint = mthing;
    }

    /// A thinker for metal spark/puff, typically used for gun-strikes against
    /// walls or non-fleshy things.
    pub(crate) fn spawn_puff(
        x: FixedT,
        y: FixedT,
        z: FixedT,
        attack_range: FixedT,
        level: &mut LevelState,
    ) {
        let z = z + (p_random() - p_random()) / 64;
        let mobj = MapObject::spawn_map_object(x, y, z, MapObjKind::MT_PUFF, level);
        let mobj = unsafe { &mut *mobj };
        mobj.momz = FixedT::ONE;
        mobj.tics -= p_random() & 3;

        if mobj.tics < 1 {
            mobj.tics = 1;
        }

        if attack_range == MELEERANGE {
            mobj.set_state(StateNum::PUFF3);
        }
    }

    /// Blood! In a game-exe!
    pub(crate) fn spawn_blood(
        x: FixedT,
        y: FixedT,
        z: FixedT,
        damage: i32,
        level: &mut LevelState,
    ) {
        let z_adj = z + (p_random() - p_random()) / 64;
        // BSP boundary: spawn_map_object takes f32/i32
        let mobj = MapObject::spawn_map_object(x, y, z_adj, MapObjKind::MT_BLOOD, level);
        let mobj = unsafe { &mut *mobj };
        mobj.momz = 2.into();
        mobj.tics -= p_random() & 3;

        if mobj.tics < 1 {
            mobj.tics = 1;
        }

        if (9..=12).contains(&damage) {
            mobj.set_state(StateNum::BLOOD2);
        } else if damage < 9 {
            mobj.set_state(StateNum::BLOOD3);
        }
    }

    /// A thinker for shooty blowy things.
    ///
    /// Doom function name is `P_SpawnPlayerMissile`
    pub(crate) fn spawn_player_missile(
        source: &mut MapObject,
        kind: MapObjKind,
        level: &mut LevelState,
    ) {
        // OG: aim from source (player) BEFORE spawning missile
        let orig_angle = source.angle;
        let mut an = orig_angle;
        // OG uses 16*64*FRACUNIT (not MISSILERANGE which is 32*64*FRACUNIT)
        let distance: FixedT = (16 * 64_i32).into();
        let mut bsp_trace = source.get_shoot_bsp_trace(distance);
        let mut slope = source.aim_line_attack(distance, &mut bsp_trace);

        if slope.is_none() {
            an = an + Angle::from_bam(1 << 26);
            source.angle = an;
            slope = source.aim_line_attack(distance, &mut bsp_trace);
            if slope.is_none() {
                an = an - Angle::from_bam(2 << 26);
                source.angle = an;
                slope = source.aim_line_attack(distance, &mut bsp_trace);
            }
            if slope.is_none() {
                an = orig_angle;
            }
        }
        // Restore source angle (OG doesn't modify source->angle)
        source.angle = orig_angle;

        let x = source.x;
        let y = source.y;
        let z = source.z + 32;

        let mobj = MapObject::spawn_map_object(x, y, z, kind, level);
        let mobj = unsafe { &mut *mobj };

        if !matches!(mobj.info.seesound, SfxName::None | SfxName::NumSfx) {
            mobj.start_sound(mobj.info.seesound);
        }

        mobj.target = Some(source.thinker);
        mobj.angle = an;
        let bam = an.to_bam();
        let speed = FixedT::from_fixed(mobj.info.speed);
        mobj.momx = speed.fixed_mul(FixedT::cos_bam(bam));
        mobj.momy = speed.fixed_mul(FixedT::sin_bam(bam));
        mobj.momz = slope
            .map(|s| speed.fixed_mul(s.aimslope))
            .unwrap_or(FixedT::ZERO);
        mobj.check_missile_spawn();
    }

    /// A thinker for shooty blowy things.
    ///
    /// Doom function name is `P_SpawnMissile`
    pub(crate) fn spawn_missile<'a>(
        source: &mut MapObject,
        target: &mut MapObject,
        kind: MapObjKind,
        level: &mut LevelState,
    ) -> &'a mut Self {
        let x = source.x;
        let y = source.y;
        let z = source.z + 32;

        let mobj = MapObject::spawn_map_object(x, y, z, kind, level);
        let mobj = unsafe { &mut *mobj };

        if !matches!(mobj.info.seesound, SfxName::None | SfxName::NumSfx) {
            mobj.start_sound(mobj.info.seesound);
        }

        let dx = target.x - source.x;
        let dy = target.y - source.y;
        mobj.angle = Angle::from_bam(r_point_to_angle(dx, dy));
        // fuzzy player
        if target.flags.contains(MapObjFlag::Shadow) {
            let fuzz = ((p_random() - p_random()) << 20) as u32;
            mobj.angle = mobj.angle + Angle::from_bam(fuzz);
        }

        mobj.target = Some(source.thinker);
        let bam = mobj.angle.to_bam();
        let speed = FixedT::from_fixed(mobj.info.speed);
        mobj.momx = speed.fixed_mul(FixedT::cos_bam(bam));
        mobj.momy = speed.fixed_mul(FixedT::sin_bam(bam));
        // OG: dist = P_AproxDistance(...) / th->info->speed (plain int div)
        //     momz = (dest->z - source->z) / dist (plain int div)
        let dx = target.x - source.x;
        let dy = target.y - source.y;
        let adist = p_aprox_distance(dx, dy);
        let mut dist = adist.to_fixed_raw() / mobj.info.speed;
        if dist < 1 {
            dist = 1;
        }
        mobj.momz = FixedT::from_fixed((target.z - source.z).to_fixed_raw() / dist);

        mobj.check_missile_spawn();
        mobj
    }

    fn check_missile_spawn(&mut self) {
        self.tics -= p_random() & 3;

        if self.tics < 1 {
            self.tics = 1;
        }

        // OG: mo->x += (mo->momx >> 1)
        self.x += self.momx.shr(1);
        self.y += self.momy.shr(1);
        self.z += self.momz.shr(1);

        if !self.p_try_move(self.x, self.y, &mut SubSectorMinMax::default()) {
            self.p_explode_missile();
        }
    }

    /// P_SpawnMobj
    ///
    /// The callee is expected to handle adding the thinker with P_AddThinker,
    /// and inserting in to the level thinker container (differently to
    /// doom).
    ///
    /// The Z position is used to determine if the object should spawn on the
    /// floor or ceiling
    pub(crate) fn spawn_map_object(
        x: FixedT,
        y: FixedT,
        z: FixedT,
        kind: MapObjKind,
        level: &mut LevelState,
    ) -> *mut MapObject {
        let info = MOBJINFO[kind as usize];
        let reactiontime = if level.options.skill != Skill::Nightmare {
            info.reactiontime
        } else {
            0
        };

        // do not set the state with P_SetMobjState,
        // because action routines can not be called yet
        let state = &STATES[info.spawnstate as usize];

        let mobj = MapObject::new(x, y, z, reactiontime, kind, info, state, level);

        let thinker = MapObject::create_thinker(ThinkerData::MapObject(mobj), MapObject::think);

        // P_AddThinker(&thing->thinker);
        if let Some(ptr) = level.thinkers.push::<MapObject>(thinker) {
            let thing = ptr.mobj_mut();
            unsafe {
                // Sets the subsector link and links in sector
                thing.set_thing_position();
                if !thing.subsector.is_null() {
                    // Now that we have a subsector this is safe
                    thing.floorz =
                        FixedT::from_fixed(thing.subsector.sector.floorheight.to_fixed_raw());
                    thing.ceilingz =
                        FixedT::from_fixed(thing.subsector.sector.ceilingheight.to_fixed_raw());

                    if z == ONFLOORZ {
                        thing.z = thing.floorz;
                    } else if z == ONCEILINGZ {
                        thing.z = thing.ceilingz - FixedT::from_f32(info.height);
                    }
                    thing.prev_z = thing.z;
                } else {
                    warn!("Thing {:?} didn't get a subsector", kind);
                }
                return thing;
            }
        }

        panic!(
            "P_SpawnMapThing: Could not spawn type {:?} at ({}, {}): out of memory",
            kind, x, y
        );
    }

    /// P_SetMobjState
    pub(crate) fn set_state(&mut self, state: StateNum) -> bool {
        // OG Doom loops through states with tics==0, calling their action functions
        let mut state = state;
        loop {
            if matches!(state, StateNum::None) {
                self.state = &STATES[StateNum::None as usize];
                self.remove();
                return false;
            }

            let st = &STATES[state as usize];
            self.state = st;
            self.tics = st.tics;
            self.sprite = st.sprite;
            self.frame = st.frame;

            // Call action functions when the state is set
            if let Some(f) = st.action.resolve_actor() {
                f(self);
            }

            state = st.next_state;
            if self.tics != 0 {
                break;
            }
        }

        true
    }

    /// P_UnsetThingPosition, unlink the thing from the sector and blockmap
    ///
    /// # Safety
    /// Thing must have had a SubSector set on creation.
    pub(crate) unsafe fn unset_thing_position(&mut self) {
        if !MOBJINFO[self.kind as usize]
            .flags
            .contains(MapObjFlag::Nosector)
        {
            let mut ss = self.subsector.clone();
            unsafe {
                ss.sector.remove_from_thinglist(self.thinker_mut());
            }
        }

        if !MOBJINFO[self.kind as usize]
            .flags
            .contains(MapObjFlag::Noblockmap)
        {
            // Unlink from blockmap chain
            if let Some(next) = self.b_next {
                unsafe {
                    (*next).b_prev = self.b_prev;
                }
            }
            if let Some(prev) = self.b_prev {
                unsafe {
                    (*prev).b_next = self.b_next;
                }
            } else {
                // Was head of chain — update blocklinks
                let level = unsafe { &mut *self.level };
                let bm = level.level_data.blockmap();
                let bx = (self.x.to_fixed_raw() - bm.x_origin) >> MAPBLOCKSHIFT;
                let by = (self.y.to_fixed_raw() - bm.y_origin) >> MAPBLOCKSHIFT;
                if bx >= 0 && bx < bm.columns && by >= 0 && by < bm.rows {
                    let idx = (by * bm.columns + bx) as usize;
                    level.blocklinks[idx] = self.b_next;
                }
            }
            self.b_next = None;
            self.b_prev = None;
        }
    }

    /// P_SetThingPosition, link the thing into the sector and blockmap
    ///
    /// # Safety
    /// Thing must have had a SubSector set on creation.
    pub(crate) unsafe fn set_thing_position(&mut self) {
        let level = unsafe { &mut *self.level };
        let mut subsector = level.level_data.point_in_subsector(self.x, self.y);
        if !MOBJINFO[self.kind as usize]
            .flags
            .contains(MapObjFlag::Nosector)
        {
            unsafe { subsector.sector.add_to_thinglist(self.thinker) }
        }
        self.subsector = subsector;

        if !MOBJINFO[self.kind as usize]
            .flags
            .contains(MapObjFlag::Noblockmap)
        {
            let level = unsafe { &mut *self.level };
            let bm = level.level_data.blockmap();
            let bx = (self.x.to_fixed_raw() - bm.x_origin) >> MAPBLOCKSHIFT;
            let by = (self.y.to_fixed_raw() - bm.y_origin) >> MAPBLOCKSHIFT;
            if bx >= 0 && bx < bm.columns && by >= 0 && by < bm.rows {
                let idx = (by * bm.columns + bx) as usize;
                self.b_prev = None;
                self.b_next = level.blocklinks[idx];
                if let Some(head) = level.blocklinks[idx] {
                    unsafe {
                        (*head).b_prev = Some(self);
                    }
                }
                level.blocklinks[idx] = Some(self);
            } else {
                self.b_next = None;
                self.b_prev = None;
            }
        }
    }

    /// P_RemoveMobj
    pub(crate) fn remove(&mut self) {
        // Respawn specials for nightmare/deathmatch
        if (self.flags.contains(MapObjFlag::Special) && !self.flags.contains(MapObjFlag::Dropped))
            && (self.kind != MapObjKind::MT_INV && self.kind != MapObjKind::MT_INS)
            && (self.level().options.respawn_monsters || self.level().options.deathmatch != 0)
        {
            let time = self.level().level_time;
            let respawn = self.spawnpoint;
            self.level_mut().respawn_queue.push_front((time, respawn));
        }

        unsafe {
            self.unset_thing_position();
        }
        // TODO: StopSound(thing);
        self.thinker_mut().mark_remove();
    }

    /// Takes a valid thing and adjusts the thing->floorz, thing->ceilingz, and
    /// possibly thing->z. This is called for all nearby monsters whenever a
    /// sector changes height. If the thing doesn't fit, the z will be set
    /// to the lowest value and false will be returned.
    ///
    /// Doom function name `P_ThingHeightClip`
    fn height_clip(&mut self) -> bool {
        let mut ctrl = SubSectorMinMax::default();
        self.p_check_position(self.x, self.y, &mut ctrl);
        let on_floor = self.z == self.floorz;
        self.floorz = ctrl.min_floor_z;
        self.ceilingz = ctrl.max_ceil_z;

        if on_floor {
            self.z = self.floorz;
        } else if self.z + self.height > self.ceilingz {
            self.z = self.ceilingz - self.height;
        }

        if self.ceilingz - self.floorz < self.height {
            return false;
        }
        true
    }

    /// PIT_ChangeSector
    ///
    /// Returns true to indicate checking should continue
    pub(crate) fn pit_change_sector(&mut self, no_fit: &mut bool, crush_change: bool) -> bool {
        if self.height_clip() {
            return true;
        }

        if self.health <= 0 {
            self.set_state(StateNum::GIBS);
            self.height = FixedT::ZERO;
            self.radius = FixedT::ZERO;
            return true;
        }

        // crunch dropped items
        if self.flags.contains(MapObjFlag::Dropped) {
            self.remove();
            return true;
        }

        if !self.flags.contains(MapObjFlag::Shootable) {
            // assume it is bloody gibs or something
            return true;
        }

        *no_fit = true;

        let level_time = unsafe { (*self.level).level_time };

        if crush_change && level_time & 3 == 0 {
            debug!("Crushing!");
            self.p_take_damage(None, None, 10);
            let mobj = MapObject::spawn_map_object(
                self.x,
                self.y,
                (self.z + self.height) / 2,
                MapObjKind::MT_BLOOD,
                unsafe { &mut *self.level },
            );
            unsafe {
                // OG: mo->momx = (P_Random() - P_Random()) << 12
                (*mobj).momx = FixedT::from_fixed(p_subrandom() << 12);
                (*mobj).momy = FixedT::from_fixed(p_subrandom() << 12);
            }
        }

        true
    }

    pub(crate) fn start_sound(&self, sfx: SfxName) {
        unsafe {
            (*self.level).start_sound(
                sfx,
                self.x.to_f32(),
                self.y.to_f32(),
                self as *const Self as usize, /* pointer cast as a UID */
            )
        }
    }

    /// P_NightmareRespawn
    pub fn nightmare_respawn(&mut self) {
        let sp_x = FixedT::from(self.spawnpoint.x as i32);
        let sp_y = FixedT::from(self.spawnpoint.y as i32);
        let mut ctrl = SubSectorMinMax::default();
        if !self.p_check_position(sp_x, sp_y, &mut ctrl) {
            return;
        }

        let ss = self.level_mut().level_data.point_in_subsector(sp_x, sp_y);
        let floor = ss.sector.floorheight.to_i32();
        let fog = unsafe {
            &mut *MapObject::spawn_map_object(
                sp_x,
                sp_y,
                floor.into(),
                MapObjKind::MT_TFOG,
                self.level_mut(),
            )
        };
        fog.start_sound(SfxName::Itmbk);

        let mthing = self.spawnpoint;

        let z = if self.info.flags.contains(MapObjFlag::Spawnceiling) {
            ONCEILINGZ
        } else {
            ONFLOORZ
        };

        let thing = unsafe {
            &mut *MapObject::spawn_map_object(sp_x, sp_y, z.into(), self.kind, self.level_mut())
        };
        thing.angle = Angle::from_bam(ANG45.wrapping_mul((mthing.angle as u32) / 45));
        thing.spawnpoint = mthing;
        thing.reactiontime = 18;
        if mthing.flags & MTF_AMBUSH != 0 {
            self.flags.insert(MapObjFlag::Ambush);
        }

        self.remove();
        dbg!();
    }
}

impl Think for MapObject {
    fn think(object: &mut Thinker, level: &mut LevelState) -> bool {
        let this = object.mobj_mut();
        #[cfg(feature = "null_check")]
        if this.thinker.is_null() {
            std::panic!("MapObject thinker was null");
        }

        // Save position for rendering interpolation
        this.prev_x = this.x;
        this.prev_y = this.y;
        this.prev_z = this.z;

        if !this.momx.is_zero() || !this.momy.is_zero() || this.flags.contains(MapObjFlag::Skullfly)
        {
            this.p_xy_movement();

            if this.thinker_mut().should_remove() {
                return true; // thing was removed
            }
        }

        if !(this.z - this.floorz).is_zero() || !this.momz.is_zero() {
            this.p_z_movement();
        }

        // cycle through states,
        // calling action functions at transitions
        if this.tics != -1 {
            this.tics -= 1;

            // you can cycle through multiple states in a tic
            if this.tics < 1 && !this.set_state(this.state.next_state) {
                return true;
            } // freed itself
        } else {
            // The corpse is still hanging around like a bad smell since it
            // is a thinker. So...
            // check for nightmare respawn, which will remove *this* if good
            if !this.flags.contains(MapObjFlag::Countkill) {
                return false;
            }
            if !level.options.respawn_monsters {
                return false;
            }
            this.movecount += 1;

            if this.movecount < 12 * TICRATE {
                return false;
            }
            if (level.level_time & 31) != 0 {
                return false;
            }
            if p_random() > 4 {
                return false;
            }
            this.nightmare_respawn();
        }
        false
    }

    fn set_thinker_ptr(&mut self, ptr: *mut Thinker) {
        self.thinker = ptr;
    }

    fn thinker_mut(&mut self) -> &mut Thinker {
        #[cfg(feature = "null_check")]
        if self.thinker.is_null() {
            std::panic!("MapObject thinker was null");
        }
        unsafe { &mut *self.thinker }
    }

    fn thinker(&self) -> &Thinker {
        #[cfg(feature = "null_check")]
        if self.thinker.is_null() {
            std::panic!("MapObject thinker was null");
        }
        unsafe { &*self.thinker }
    }
}

// pub(crate) fn kind_from_doomednum(mthing: &WadThing) -> MapObjKind {
//     // find which type to spawn
//     let mut i = 0;
//     for n in 0..MapObjKind::Count as u16 {
//         if mthing.kind == MOBJINFO[n as usize].doomednum as i16 {
//             i = n;
//             break;
//         }
//     }

//     if i == MapObjKind::Count as u16 {
//         error!(
//             "P_SpawnMapThing: Unknown type {} at ({}, {})",
//             mthing.kind, mthing.x, mthing.y
//         );
//     }

//     MapObjKind::from(i)
// }
