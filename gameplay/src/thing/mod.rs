//! The bulk of map entities, typically shown as sprites. Things like monsters,
//! giblets, rockets and plasma shots etc, items.
//!
//! `MapObject` is also used for moving doors, platforms, floors, and ceilings.

mod interact;
pub use interact::*;
mod movement;
pub use movement::*;
use sound_traits::SfxName;
pub(crate) mod enemy;
mod shooting;

use std::fmt::Debug;
use std::ptr::null_mut;

use self::movement::SubSectorMinMax;

use crate::doom_def::{MELEERANGE, MISSILERANGE, MTF_SINGLE_PLAYER};
use crate::level::Level;
use crate::thinker::{Think, Thinker, ThinkerData};
use crate::{MapPtr, Skill};
use glam::Vec2;
use log::{debug, error, trace, warn};
use wad::types::WadThing;

use crate::angle::Angle;
use crate::doom_def::{ActFn, MAXPLAYERS, MTF_AMBUSH, ONCEILINGZ, ONFLOORZ, TICRATE, VIEWHEIGHT};
use crate::info::{MapObjInfo, MapObjKind, SpriteNum, State, StateNum, MOBJINFO, STATES};
use crate::level::map_defs::SubSector;
use crate::player::{Player, PlayerState};
use crate::utilities::{p_random, p_subrandom, point_to_angle_2, BestSlide};

//static MOBJ_CYCLE_LIMIT: u32 = 1000000;
#[derive(Debug, PartialEq)]
pub enum MapObjFlag {
    /// Call P_SpecialThing when touched.
    Special = 1,
    /// Blocks.
    Solid = 2,
    /// Can be hit.
    Shootable = 4,
    /// Don't use the sector links (invisible but touchable).
    Nosector = 8,
    /// Don't use the block links (inert but displayable)
    Noblockmap = 16,
    /// Not to be activated by sound, deaf monster.
    Ambush = 32,
    /// Will try to attack right back.
    Justhit = 64,
    /// Will take at least one step before attacking.
    Justattacked = 128,
    /// On level spawning (initial position), hang from ceiling instead of stand
    /// on floor.
    Spawnceiling = 256,
    /// Don't apply gravity (every tic), that is, object will float, keeping
    /// current height  or changing it actively.
    Nogravity = 512,
    /// This allows jumps from high places.
    Dropoff = 0x400,
    /// For players, will pick up items.
    Pickup = 0x800,
    /// Player cheat. ???
    Noclip = 0x1000,
    /// Player: keep info about sliding along walls.
    Slide = 0x2000,
    /// Allow moves to any height, no gravity. For active floaters, e.g.
    /// cacodemons, pain elementals.
    Float = 0x4000,
    /// Don't cross lines ??? or look at heights on teleport.
    Teleport = 0x8000,
    /// Don't hit same species, explode on block. Player missiles as well as
    /// fireballs of various kinds.
    Missile = 0x10000,
    /// Dropped by a demon, not level spawned. E.g. ammo clips dropped by dying
    /// former humans.
    Dropped = 0x20000,
    /// Use fuzzy draw (shadow demons or spectres),  temporary player
    /// invisibility powerup.
    Shadow = 0x40000,
    /// Flag: don't bleed when shot (use puff),  barrels and shootable furniture
    /// shall not bleed.
    Noblood = 0x80000,
    /// Don't stop moving halfway off a step, that is, have dead bodies slide
    /// down all the way.
    Corpse = 0x100000,
    /// Floating to a height for a move, ??? don't auto float to target's
    /// height.
    Infloat = 0x200000,
    /// On kill, count this enemy object towards intermission kill total. Happy
    /// gathering.
    Countkill = 0x400000,
    /// On picking up, count this item object towards intermission item total.
    Countitem = 0x800000,
    /// Special handling: skull in flight. Neither a cacodemon nor a missile.
    Skullfly = 0x1000000,
    /// Don't spawn this object in death match mode (e.g. key cards).
    Notdmatch = 0x2000000,
    /// Player sprites in multiplayer modes are modified using an internal color
    /// lookup table for re-indexing. If 0x4 0x8 or 0xc, use a translation
    /// table for player colormaps
    Translation = 0xC000000,
    /// Hmm ???.
    Transshift = 26,
}

pub struct MapObject {
    /// `MapObject` is owned by the `Thinker`. If the `MapObject` is ever moved
    /// out of the `Thinker` then you must update sector thing lists and self
    /// linked list. This is a pointer to the `Thinker` storage.
    pub(super) thinker: *mut Thinker,
    /// Specific to Doom II. These are pointers to targets that the
    /// final boss shoots demon spawn cubes towards. It is expected that
    /// because these are level items they will never shift their memory
    /// location. The raw pointers here are to map entities that never
    /// move/delete themselves.
    pub(super) boss_targets: Vec<*mut Thinker>,
    /// Specific to Doom II. The current target (spawn point for demons)
    pub(super) boss_target_on: usize,
    /// Info for drawing: position.
    pub xy: Vec2,
    pub z: f32,
    // More drawing info: to determine current sprite.
    /// orientation
    pub angle: Angle,
    /// used to find patch_t and flip value
    pub sprite: SpriteNum,
    /// might be ORed with FF_FULLBRIGHT
    pub frame: u32,
    /// Link to the next `Thinker` in this sector. You can think of this as a
    /// separate Linked List to the `Thinker` linked list used in storage. I
    /// does mean that you will need to unlink an object both here, and in the
    /// Thinker storage if removing one.
    pub(super) s_next: Option<*mut Thinker>,
    /// Link to the previous `Thinker` in this sector
    pub(super) s_prev: Option<*mut Thinker>,
    /// The subsector this object is currently in. When a map object is spawned
    /// `set_thing_position()` is called which then sets this to a valid
    /// subsector, making this safe in 99% of cases.
    pub subsector: MapPtr<SubSector>,
    /// The closest interval over all contacted Sectors.
    pub(crate) floorz: f32,
    pub(crate) ceilingz: f32,
    /// For movement checking.
    pub(crate) radius: f32,
    pub(crate) height: f32,
    /// Momentum, used to update position.
    pub(crate) momxy: Vec2,
    pub(crate) momz: f32,
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
    pub state: &'static State,
    pub flags: u32,
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
    player: Option<*mut Player>,
    /// Player number last looked for, 1-4 (does not start at 0)
    lastlook: i32,
    /// For nightmare respawn.
    pub(crate) spawnpoint: WadThing,
    // Thing being chased/attacked for tracers.
    // struct mobj_s*	tracer;
    /// Every map object needs a link to the level structure to read various
    /// level elements and possibly change some (sector links for example).
    pub(crate) level: *mut Level,
}

impl Debug for MapObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapObject")
            .field("xy", &self.xy)
            .field("z", &self.z)
            .field("angle", &self.angle)
            .field("sprite", &self.sprite)
            .field("frame", &self.frame)
            .field("floorz", &self.floorz)
            .field("ceilingz", &self.ceilingz)
            .field("radius", &self.radius)
            .field("height", &self.height)
            .field("momxy", &self.momxy)
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
        x: f32,
        y: f32,
        z: i32,
        reactiontime: i32,
        kind: MapObjKind,
        info: MapObjInfo,
        state: &'static State,
        level: *mut Level,
    ) -> Self {
        Self {
            thinker: null_mut(),
            boss_targets: Vec::new(),
            boss_target_on: 0,
            player: None,
            xy: Vec2::new(x, y),
            z: z as f32,
            angle: Angle::new(0.0),
            sprite: state.sprite,
            frame: state.frame,
            floorz: 0.0,
            ceilingz: 0.0,
            radius: info.radius,
            height: info.height,
            momxy: Vec2::default(),
            momz: 0.0,
            valid_count: 0,
            flags: info.flags,
            health: info.spawnhealth,
            tics: state.tics,
            movedir: MoveDir::North,
            movecount: 0,
            best_slide: BestSlide::default(),
            reactiontime,
            threshold: 0,
            lastlook: p_random() % MAXPLAYERS as i32,
            spawnpoint: WadThing::default(),
            target: None,
            tracer: None,
            s_next: None,
            s_prev: None,
            subsector: unsafe { MapPtr::new_null() },
            state,
            info,
            kind,
            level,
        }
    }

    pub(crate) fn level(&self) -> &Level {
        #[cfg(feature = "null_check")]
        if self.level.is_null() {
            std::panic!("MapObject level pointer was null");
        }
        unsafe { &*self.level }
    }

    pub(crate) fn level_mut(&mut self) -> &mut Level {
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
        level: &mut Level,
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
            mthing.x as f32,
            mthing.y as f32,
            ONFLOORZ,
            MapObjKind::MT_PLAYER,
            level,
        );

        // set color translations for player sprites

        let mobj_ptr_mut = unsafe { &mut *mobj };
        if mthing.kind > 1 {
            mobj_ptr_mut.flags |= (mthing.kind as u32 - 1) << MapObjFlag::Transshift as u8;
        }

        // TODO: check this angle stuff
        mobj_ptr_mut.angle = Angle::new((mthing.angle as f32).to_radians());
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
        player.viewheight = VIEWHEIGHT;

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
        level: &mut Level,
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
            && MOBJINFO[i as usize].flags & MapObjFlag::Notdmatch as u32 != 0
        {
            return;
        }

        // TODO: don't spawn any monsters if -nomonsters
        let kind = MapObjKind::from(i);
        if no_monsters
            && (kind == MapObjKind::MT_SKULL
                || MOBJINFO[i as usize].flags & MapObjFlag::Countkill as u32 != 0)
        {
            return;
        }

        let x = mthing.x as f32;
        let y = mthing.y as f32;
        let z = if MOBJINFO[i as usize].flags & MapObjFlag::Spawnceiling as u32 != 0 {
            ONCEILINGZ
        } else {
            ONFLOORZ
        };

        let mobj = MapObject::spawn_map_object(x, y, z, MapObjKind::from(i), level);
        let mobj = unsafe { &mut *mobj };
        if mobj.tics > 0 {
            mobj.tics = 1 + (p_random() % mobj.tics);
        }
        if mobj.flags & MapObjFlag::Countkill as u32 != 0 {
            level.total_level_kills += 1;
        }
        if mobj.flags & MapObjFlag::Countitem as u32 != 0 {
            level.total_level_items += 1;
        }

        // TODO: check the angle is correct
        mobj.angle = Angle::new((mthing.angle as f32).to_radians());
        if mthing.flags & MTF_AMBUSH != 0 {
            mobj.flags |= MapObjFlag::Ambush as u32;
        }

        mobj.spawnpoint = mthing;
    }

    /// A thinker for metal spark/puff, typically used for gun-strikes against
    /// walls or non-fleshy things.
    pub(crate) fn spawn_puff(x: f32, y: f32, z: i32, attack_range: f32, level: &mut Level) {
        let mobj = MapObject::spawn_map_object(x, y, z, MapObjKind::MT_PUFF, level);
        let mobj = unsafe { &mut *mobj };
        mobj.momz = 1.0;
        mobj.tics -= p_random() & 3;

        if mobj.tics < 1 {
            mobj.tics = 1;
        }

        if attack_range == MELEERANGE {
            mobj.set_state(StateNum::PUFF3);
        }
    }

    /// Blood! In a game-exe!
    pub(crate) fn spawn_blood(x: f32, y: f32, mut z: i32, damage: f32, level: &mut Level) {
        z += (p_random() - p_random()) / 64;
        let mobj = MapObject::spawn_map_object(x, y, z, MapObjKind::MT_BLOOD, level);
        let mobj = unsafe { &mut *mobj };
        mobj.momz = 2.0;
        mobj.tics -= p_random() & 3;

        if mobj.tics < 1 {
            mobj.tics = 1;
        }

        if (9.0..=12.0).contains(&damage) {
            mobj.set_state(StateNum::BLOOD2);
        } else if damage < 9.0 {
            mobj.set_state(StateNum::BLOOD3);
        }
    }

    /// A thinker for shooty blowy things.
    ///
    /// Doom function name is `P_SpawnPlayerMissile`
    pub(crate) fn spawn_player_missile(
        source: &mut MapObject,
        kind: MapObjKind,
        level: &mut Level,
    ) {
        let x = source.xy.x;
        let y = source.xy.y;
        let z = source.z + 32.0;

        let mobj = MapObject::spawn_map_object(x, y, z as i32, kind, level);
        let mobj = unsafe { &mut *mobj };
        mobj.angle = source.angle;

        let mut bsp_trace = mobj.get_shoot_bsp_trace(MISSILERANGE);
        let mut slope = mobj.aim_line_attack(MISSILERANGE, &mut bsp_trace);

        if slope.is_none() {
            mobj.angle += 5.625f32.to_radians();
            slope = mobj.aim_line_attack(MISSILERANGE, &mut bsp_trace);
            if slope.is_none() {
                mobj.angle -= 11.25f32.to_radians();
                slope = mobj.aim_line_attack(MISSILERANGE, &mut bsp_trace);
            }
            if slope.is_none() {
                mobj.angle = source.angle;
            }
        }

        if !matches!(mobj.info.seesound, SfxName::None | SfxName::NumSfx) {
            mobj.start_sound(mobj.info.seesound);
        }

        mobj.target = Some(source.thinker);
        mobj.momxy = mobj.angle.unit() * mobj.info.speed;
        mobj.momz = slope.map(|s| s.aimslope * mobj.info.speed).unwrap_or(0.0);
        mobj.check_missile_spawn();
    }

    /// A thinker for shooty blowy things.
    ///
    /// Doom function name is `P_SpawnMissile`
    pub(crate) fn spawn_missile<'a>(
        source: &mut MapObject,
        target: &mut MapObject,
        kind: MapObjKind,
        level: &mut Level,
    ) -> &'a mut Self {
        let x = source.xy.x;
        let y = source.xy.y;
        let z = source.z + 32.0;

        let mobj = MapObject::spawn_map_object(x, y, z as i32, kind, level);
        let mobj = unsafe { &mut *mobj };

        if !matches!(mobj.info.seesound, SfxName::None | SfxName::NumSfx) {
            mobj.start_sound(mobj.info.seesound);
        }

        mobj.angle = point_to_angle_2(target.xy, source.xy);
        // fuzzy player
        if target.flags & MapObjFlag::Shadow as u32 != 0 {
            mobj.angle += (((p_random() - p_random()) >> 4) as f32).to_radians();
        }

        mobj.target = Some(source.thinker);
        mobj.momxy = mobj.angle.unit() * mobj.info.speed;
        //thing.momz = slope.map(|s| s.aimslope * thing.info.speed).unwrap_or(0.0);
        let mut dist = mobj.xy.distance(target.xy) / mobj.info.speed;
        if dist < 1.0 {
            dist = 1.0;
        }
        mobj.momz = (target.z - source.z) / dist;

        mobj.check_missile_spawn();
        mobj
    }

    fn check_missile_spawn(&mut self) {
        self.tics -= p_random() & 3;

        if self.tics < 1 {
            self.tics = 1;
        }

        self.xy += self.momxy / 2.0;
        self.z += self.momz / 2.0;

        if !self.p_try_move(self.xy.x, self.xy.y, &mut SubSectorMinMax::default()) {
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
        x: f32,
        y: f32,
        z: i32,
        kind: MapObjKind,
        level: &mut Level,
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
                    thing.floorz = thing.subsector.sector.floorheight;
                    thing.ceilingz = thing.subsector.sector.ceilingheight;

                    if z == ONFLOORZ {
                        thing.z = thing.floorz;
                    } else if z == ONCEILINGZ {
                        thing.z = thing.ceilingz - info.height;
                    }
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
        // Using the Heretic/Hexen style, no loop

        // let mut cycle_counter = 0;
        // loop {
        if matches!(state, StateNum::None) {
            self.state = &STATES[StateNum::None as usize]; //(state_t *)NULL;
            self.remove();
            return false;
        }

        let st = &STATES[state as usize];
        self.state = st;
        self.tics = st.tics;
        self.sprite = st.sprite;
        self.frame = st.frame;

        // Modified handling.
        // Call action functions when the state is set
        if let ActFn::A(f) = st.action {
            f(self);
        }

        //     state = st.next_state;
        //     cycle_counter += 1;
        //     if cycle_counter > MOBJ_CYCLE_LIMIT {
        //         panic!(
        //             "P_SetMobjState: Infinite state cycle detected! {:?}",
        //             self.info
        //         );
        //     }

        //     if self.tics != 0 {
        //         break;
        //     }
        // }

        true
    }

    /// P_UnsetThingPosition, unlink the thing from the sector
    ///
    /// # Safety
    /// Thing must have had a SubSector set on creation.
    pub(crate) unsafe fn unset_thing_position(&mut self) {
        if MOBJINFO[self.kind as usize].flags & MapObjFlag::Nosector as u32 == 0 {
            let mut ss = self.subsector.clone();
            ss.sector.remove_from_thinglist(self.thinker_mut());
        }
    }

    /// P_SetThingPosition, unlink the thing from the sector
    ///
    /// # Safety
    /// Thing must have had a SubSector set on creation.
    pub(crate) unsafe fn set_thing_position(&mut self) {
        let level = &mut *self.level;
        let mut subsector = level.map_data.point_in_subsector_raw(self.xy);
        if MOBJINFO[self.kind as usize].flags & MapObjFlag::Nosector as u32 == 0 {
            subsector.sector.add_to_thinglist(self.thinker)
        }
        self.subsector = subsector;
    }

    /// P_RemoveMobj
    pub(crate) fn remove(&mut self) {
        // Respawn specials for nightmare/deathmatch
        if (self.flags & MapObjFlag::Special as u32 != 0
            && self.flags & MapObjFlag::Dropped as u32 == 0)
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
        // how efficient is this really?
        self.p_check_position(self.xy, &mut ctrl);
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
            self.height = 0.0;
            self.radius = 0.0;
            return true;
        }

        // crunch dropped items
        if self.flags & MapObjFlag::Dropped as u32 != 0 {
            self.remove();
            return true;
        }

        if self.flags & MapObjFlag::Shootable as u32 == 0 {
            // assume it is bloody gibs or something
            return true;
        }

        *no_fit = true;

        let level_time = unsafe { (*self.level).level_time };

        if crush_change && level_time & 3 == 0 {
            debug!("Crushing!");
            self.p_take_damage(None, None, false, 10);
            let mobj = MapObject::spawn_map_object(
                self.xy.x,
                self.xy.y,
                (self.z + self.height) as i32 / 2,
                MapObjKind::MT_BLOOD,
                unsafe { &mut *self.level },
            );
            unsafe {
                (*mobj).momxy.x = p_subrandom() as f32 * 0.6; // P_SubRandom() << 12;
                (*mobj).momxy.y = p_subrandom() as f32 * 0.6;
            }
        }

        true
    }

    pub(crate) fn start_sound(&self, sfx: SfxName) {
        unsafe {
            (*self.level).start_sound(
                sfx,
                self.xy.x,
                self.xy.y,
                self as *const Self as usize, /* pointer cast as a UID */
            )
        }
    }

    /// P_NightmareRespawn
    pub fn nightmare_respawn(&mut self) {
        let xy = Vec2::new(self.spawnpoint.x as f32, self.spawnpoint.y as f32);
        let mut ctrl = SubSectorMinMax::default();
        if !self.p_check_position(xy, &mut ctrl) {
            return;
        }

        let ss = self.level_mut().map_data.point_in_subsector(xy);
        let floor = ss.sector.floorheight as i32;
        let fog = unsafe {
            &mut *MapObject::spawn_map_object(
                xy.x,
                xy.y,
                floor,
                MapObjKind::MT_TFOG,
                self.level_mut(),
            )
        };
        fog.start_sound(SfxName::Itmbk);

        let mthing = self.spawnpoint;

        let z = if self.info.flags & MapObjFlag::Spawnceiling as u32 != 0 {
            ONCEILINGZ
        } else {
            ONFLOORZ
        };

        let thing = unsafe {
            &mut *MapObject::spawn_map_object(xy.x, xy.y, z, self.kind, self.level_mut())
        };
        thing.angle = Angle::new((mthing.angle as f32).to_radians());
        thing.spawnpoint = mthing;
        thing.reactiontime = 18;
        if mthing.flags & MTF_AMBUSH != 0 {
            self.flags |= MapObjFlag::Ambush as u32;
        }

        self.remove();
        dbg!();
    }
}

impl Think for MapObject {
    fn think(object: &mut Thinker, level: &mut Level) -> bool {
        let this = object.mobj_mut();
        #[cfg(feature = "null_check")]
        if this.thinker.is_null() {
            std::panic!("MapObject thinker was null");
        }

        if this.momxy.x != 0.0 || this.momxy.y != 0.0 || MapObjFlag::Skullfly as u32 != 0 {
            this.p_xy_movement();

            if this.thinker_mut().should_remove() {
                return true; // thing was removed
            }
        }

        if (this.z - this.floorz).abs() > f32::EPSILON || this.momz != 0.0 {
            this.p_z_movement();
        }

        // cycle through states,
        // calling action functions at transitions
        if this.tics != -1 {
            this.tics -= 1;

            // you can cycle through multiple states in a tic
            if this.tics < 0 && !this.set_state(this.state.next_state) {
                return true;
            } // freed itself
        } else {
            // The corpse is still hanging around like a bad smell since it
            // is a thinker. So...
            // check for nightmare respawn, which will remove *this* if good
            if this.flags & MapObjFlag::Countkill as u32 == 0 {
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
