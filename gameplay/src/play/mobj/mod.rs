//! The bulk of map objects. Things like monsters, giblets, rockets and plasma
//! shots etc. Items. Anything that needs to move.
//!
//! Doom source name `p_mobj`

mod interact;
pub use interact::*;
mod movement;
pub use movement::*;
use sound_traits::SfxEnum;
mod shooting;

use std::ptr::{null_mut, NonNull};

use self::movement::SubSectorMinMax;

use super::{
    player::{Player, PlayerState},
    utilities::{p_random, p_subrandom, BestSlide, ONCEILINGZ, ONFLOORZ, VIEWHEIGHT},
    Skill,
};

use crate::{
    doom_def::{MELEERANGE, MISSILERANGE, MTF_SINGLE_PLAYER},
    level::Level,
    thinker::{ObjectType, Think, Thinker},
};
use glam::Vec2;
use log::{debug, error};
use wad::lumps::WadThing;

use crate::{
    angle::Angle,
    doom_def::{MAXPLAYERS, MTF_AMBUSH, TICRATE},
    info::{ActionF, MapObjectInfo, MapObjectType, SpriteNum, State, StateNum, MOBJINFO, STATES},
    level::map_defs::SubSector,
};

//static MOBJ_CYCLE_LIMIT: u32 = 1000000;
#[derive(Debug, PartialEq)]
pub enum MapObjectFlag {
    /// Call P_SpecialThing when touched.
    Special = 1,
    /// Blocks.
    Solid = 2,
    /// Can be hit.
    Shootable = 4,
    /// Don't use the sector links (invisible but touchable).
    NoSector = 8,
    /// Don't use the block links (inert but displayable)
    NoBlockMap = 16,
    /// Not to be activated by sound, deaf monster.
    Ambush = 32,
    /// Will try to attack right back.
    JustHit = 64,
    /// Will take at least one step before attacking.
    JustAttacked = 128,
    /// On level spawning (initial position), hang from ceiling instead of stand on floor.
    SpawnCeiling = 256,
    /// Don't apply gravity (every tic), that is, object will float, keeping current height
    ///  or changing it actively.
    NoGravity = 512,
    /// This allows jumps from high places.
    DropOff = 0x400,
    /// For players, will pick up items.
    Pickup = 0x800,
    /// Player cheat. ???
    NoClip = 0x1000,
    /// Player: keep info about sliding along walls.
    Slide = 0x2000,
    /// Allow moves to any height, no gravity. For active floaters, e.g. cacodemons, pain elementals.
    Float = 0x4000,
    /// Don't cross lines ??? or look at heights on teleport.
    Teleport = 0x8000,
    /// Don't hit same species, explode on block. Player missiles as well as fireballs of various kinds.
    Missile = 0x10000,
    /// Dropped by a demon, not level spawned. E.g. ammo clips dropped by dying former humans.
    Dropped = 0x20000,
    /// Use fuzzy draw (shadow demons or spectres),  temporary player invisibility powerup.
    Shadow = 0x40000,
    /// Flag: don't bleed when shot (use puff),  barrels and shootable furniture shall not bleed.
    NoBlood = 0x80000,
    /// Don't stop moving halfway off a step, that is, have dead bodies slide down all the way.
    Corpse = 0x100000,
    /// Floating to a height for a move, ??? don't auto float to target's height.
    InFloat = 0x200000,
    /// On kill, count this enemy object towards intermission kill total. Happy gathering.
    CountKill = 0x400000,
    /// On picking up, count this item object towards intermission item total.
    CountItem = 0x800000,
    /// Special handling: skull in flight. Neither a cacodemon nor a missile.
    SkullFly = 0x1000000,
    /// Don't spawn this object in death match mode (e.g. key cards).
    NotDeathmatch = 0x2000000,
    /// Player sprites in multiplayer modes are modified using an internal color lookup table
    /// for re-indexing. If 0x4 0x8 or 0xc, use a translation table for player colormaps
    Translation = 0xc000000,
    /// Hmm ???.
    Transshift = 26,
}

pub struct MapObject {
    /// The MapObject owns the Thinker. If the MapObject moves at all then the
    /// Thinker must have its link to
    pub thinker: *mut Thinker,
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
    /// Link to the next object in this sector. This is only ever used by functions
    /// implemented on `Sector`.
    ///
    /// # Safety
    /// A reference gained through `s_next` must never outlive this `self` unless links
    /// are updated.
    pub s_next: Option<NonNull<MapObject>>,
    /// Link to the previous object in this sector.  This is only ever used by functions
    /// implemented on `Sector`
    ///
    /// # Safety
    /// A reference gained through `s_prev` must never outlive this `self` unless links
    /// are updated.
    pub s_prev: Option<NonNull<MapObject>>,
    /// The subsector this object is currently in
    pub subsector: *mut SubSector,
    /// The closest interval over all contacted Sectors.
    pub floorz: f32,
    pub ceilingz: f32,
    /// For movement checking.
    pub radius: f32,
    pub height: f32,
    /// Momentums, used to update position.
    pub momxy: Vec2,
    pub momz: f32,
    /// If == validcount, already checked.
    pub valid_count: usize,
    /// The type of object
    pub kind: MapObjectType,
    /// &mobjinfo[mobj.type]
    pub info: MapObjectInfo,
    pub tics: i32,
    /// state tic counter
    // TODO: probably only needs to be an index to the array
    //  using the enum as the indexer
    pub state: &'static State,
    pub flags: u32,
    pub health: i32,
    /// Movement direction, movement generation (zig-zagging).
    /// 0-7
    movedir: i32,
    /// when 0, select a new dir
    movecount: i32,
    /// The best slide move for a player object
    pub best_slide: BestSlide,
    /// Thing being chased/attacked (or NULL),
    /// also the originator for missiles.
    pub target: Option<*mut MapObject>,
    /// Reaction time: if non 0, don't attack yet.
    /// Used by player to freeze a bit after teleporting.
    pub reactiontime: i32,
    /// If >0, the target will be chased
    /// no matter what (even if shot)
    pub threshold: i32,
    /// Additional info record for player avatars only. Only valid if type == MT_PLAYER.
    /// RUST: If this is not `None` then the pointer is guaranteed to point to a player
    pub player: Option<*mut Player>,
    /// Player number last looked for.
    lastlook: i32,
    /// For nightmare respawn.
    spawn_point: Option<WadThing>,
    // Thing being chased/attacked for tracers.
    // struct mobj_s*	tracer;
    /// Every map object needs a link to the level structure to read various level
    /// elements and possibly change some (sector links for example).
    pub level: *mut Level,
}

impl MapObject {
    fn new(
        x: f32,
        y: f32,
        z: i32,
        reactiontime: i32,
        kind: MapObjectType,
        info: MapObjectInfo,
        state: &'static State,
        level: *mut Level,
    ) -> Self {
        Self {
            thinker: null_mut(),
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
            movedir: 0,
            movecount: 0,
            best_slide: BestSlide::default(),
            reactiontime,
            threshold: 0,
            lastlook: p_random() % MAXPLAYERS as i32,
            spawn_point: None,
            target: None,
            s_next: None,
            s_prev: None,
            subsector: null_mut(),
            state,
            info,
            kind,
            level,
        }
    }

    /// P_SpawnPlayer
    /// Called when a player is spawned on the level.
    /// Most of the player structure stays unchanged
    ///  between levels.
    ///
    /// Called in game.c
    pub fn p_spawn_player(
        mthing: &WadThing,
        level: &mut Level,
        players: &mut [Player],
        active_players: &[bool; MAXPLAYERS],
    ) {
        if mthing.kind == 0 {
            return;
        }

        // not playing?
        if !active_players[(mthing.kind - 1) as usize] {
            return;
        }

        let mut player = &mut players[0];

        if player.player_state == PlayerState::Reborn {
            player.reborn();
        }

        // Doom spawns this in it's memory manager then passes a pointer back. As fasr as I can see
        // the Player object owns this.
        let mobj = MapObject::spawn_map_object(
            mthing.x as f32,
            mthing.y as f32,
            ONFLOORZ,
            MapObjectType::MT_PLAYER,
            level,
        );

        // set color translations for player sprites

        let mobj_ptr_mut = unsafe { &mut *mobj };
        if mthing.kind > 1 {
            mobj_ptr_mut.flags = mobj_ptr_mut.flags as u32
                | (mthing.kind as u32 - 1) << MapObjectFlag::Transshift as u8;
        }

        // TODO: check this angle stuff
        mobj_ptr_mut.angle = Angle::new((mthing.angle as f32).to_radians());
        mobj_ptr_mut.health = player.health;
        mobj_ptr_mut.player = Some(player);

        player.mobj = Some(mobj);
        player.player_state = PlayerState::Live;
        player.refire = 0;
        player.message = None;
        player.damagecount = 0;
        player.bonuscount = 0;
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
        mthing: &WadThing,
        level: &mut Level,
        players: &mut [Player],
        active_players: &[bool; MAXPLAYERS],
    ) {
        // count deathmatch start positions
        if mthing.kind == 11 {
            if level.deathmatch_p.len() < level.deathmatch_starts.len() {
                level.deathmatch_p.push(*mthing);
            }
            return;
        }

        // check for players specially
        if mthing.kind <= 4 && mthing.kind != 0 {
            // save spots for respawning in network games
            level.player_starts[(mthing.kind - 1) as usize] = Some(*mthing);
            if !level.deathmatch {
                MapObject::p_spawn_player(mthing, level, players, active_players);
            }
            return;
        }

        // check for appropriate skill level
        if !level.deathmatch && mthing.flags & MTF_SINGLE_PLAYER != 0 {
            return;
        }
        let bit: i16;
        if level.game_skill == Skill::Baby {
            bit = 1;
        } else if level.game_skill == Skill::Nightmare {
            bit = 4;
        } else {
            bit = 1 << (level.game_skill as i16 - 1);
        }

        if mthing.flags & bit == 0 {
            return;
        }

        // find which type to spawn
        let mut i = 0;
        for n in 0..MapObjectType::NUMMOBJTYPES as u16 {
            if mthing.kind == MOBJINFO[n as usize].doomednum as i16 {
                i = n;
                break;
            }
        }

        if i == MapObjectType::NUMMOBJTYPES as u16 {
            error!(
                "P_SpawnMapThing: Unknown type {} at ({}, {})",
                mthing.kind, mthing.x, mthing.y
            );
        }

        // don't spawn keycards and players in deathmatch
        if level.deathmatch && MOBJINFO[i as usize].flags & MapObjectFlag::NotDeathmatch as u32 != 0
        {
            return;
        }

        // TODO: don't spawn any monsters if -nomonsters
        // if (nomonsters && (i == MT_SKULL || (mobjinfo[i].flags & COUNTKILL)))
        // {
        //     return;
        // }

        let x = mthing.x as f32;
        let y = mthing.y as f32;
        let z = if MOBJINFO[i as usize].flags & MapObjectFlag::SpawnCeiling as u32 != 0 {
            ONCEILINGZ
        } else {
            ONFLOORZ
        };

        let mobj = MapObject::spawn_map_object(x, y, z, MapObjectType::from(i), level);
        let mobj = unsafe { &mut *mobj };
        if mobj.tics > 0 {
            mobj.tics = 1 + (p_random() % mobj.tics);
        }
        if mobj.flags & MapObjectFlag::CountKill as u32 != 0 {
            level.totalkills += 1;
        }
        if mobj.flags & MapObjectFlag::CountItem as u32 != 0 {
            level.totalitems += 1;
        }

        // TODO: check the angle is correct
        mobj.angle = Angle::new((mthing.angle as f32).to_radians());
        if mthing.flags & MTF_AMBUSH != 0 {
            mobj.flags |= MapObjectFlag::Ambush as u32;
        }
    }

    /// A thinker for metal spark/puff, typically used for gun-strikes against walls or non-fleshy things.
    pub fn spawn_puff(x: f32, y: f32, z: i32, attack_range: f32, level: &mut Level) {
        let mobj = MapObject::spawn_map_object(x, y, z, MapObjectType::MT_PUFF, level);
        let mobj = unsafe { &mut *mobj };
        mobj.momz = 1.0;
        mobj.tics -= p_random() & 3;

        if mobj.tics < 1 {
            mobj.tics = 1;
        }

        if attack_range == MELEERANGE {
            mobj.set_state(StateNum::S_PUFF3);
        }
    }

    /// Blood! In a game!
    pub fn spawn_blood(x: f32, y: f32, mut z: i32, damage: f32, level: &mut Level) {
        z += (p_random() - p_random()) / 64;
        let mobj = MapObject::spawn_map_object(x, y, z, MapObjectType::MT_BLOOD, level);
        let mobj = unsafe { &mut *mobj };
        mobj.momz = 2.0;
        mobj.tics -= p_random() & 3;

        if mobj.tics < 1 {
            mobj.tics = 1;
        }

        if (9.0..=12.0).contains(&damage) {
            mobj.set_state(StateNum::S_BLOOD2);
        } else if damage < 9.0 {
            mobj.set_state(StateNum::S_BLOOD3);
        }
    }

    /// A thinker for shooty blowy things.
    ///
    /// Doom function name is `P_SpawnPlayerMissile`
    pub fn spawn_player_missile(source: &mut MapObject, kind: MapObjectType, level: &mut Level) {
        let x = source.xy.x();
        let y = source.xy.y();
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

        if !matches!(mobj.info.seesound, SfxEnum::None | SfxEnum::NumSfx) {
            mobj.start_sound(mobj.info.seesound);
        }

        mobj.target = Some(source);
        mobj.momxy = mobj.angle.unit() * mobj.info.speed;
        mobj.momz = slope.map(|s| s.aimslope * mobj.info.speed).unwrap_or(0.0);
        mobj.check_missile_spawn();
    }

    pub fn check_missile_spawn(&mut self) {
        self.tics -= p_random() & 3;

        if self.tics < 1 {
            self.tics = 1;
        }

        self.xy += self.momxy / 2.0;
        self.z += self.momz / 2.0;

        if !self.p_try_move(self.xy.x(), self.xy.y()) {
            self.p_explode_missile();
        }
    }

    /// P_SpawnMobj
    ///
    /// The callee is expected to handle adding the thinker with P_AddThinker, and
    /// inserting in to the level thinker container (differently to doom).
    ///
    // TODO: pass in a ref to the container so the obj can be added
    //  Doom calls an zmalloc function for this. Then pass a reference back for it
    pub fn spawn_map_object(
        x: f32,
        y: f32,
        z: i32,
        kind: MapObjectType,
        level: &mut Level,
    ) -> *mut MapObject {
        let info = MOBJINFO[kind as usize];
        let reactiontime = if level.game_skill != Skill::Nightmare {
            info.reactiontime
        } else {
            0
        };

        // // do not set the state with P_SetMobjState,
        // // because action routines can not be called yet
        let state = &STATES[info.spawnstate as usize];

        let mobj = MapObject::new(x, y, z, reactiontime, kind, info, state, level);

        let thinker = MapObject::create_thinker(ObjectType::MapObject(mobj), MapObject::think);

        // P_AddThinker(&mobj->thinker);
        if let Some(ptr) = level.thinkers.push::<MapObject>(thinker) {
            let thing = ptr.object_mut().mobj();
            unsafe {
                // Sets the subsector link and links in sector
                thing.set_thing_position();
                if !thing.subsector.is_null() {
                    // Now that we have a subsector this is safe
                    thing.floorz = (*thing.subsector).sector.floorheight;
                    thing.ceilingz = (*thing.subsector).sector.ceilingheight;

                    if z == ONFLOORZ {
                        thing.z = thing.floorz;
                    } else if z == ONCEILINGZ {
                        thing.z = thing.ceilingz - info.height;
                    }
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
    pub fn set_state(&mut self, state: StateNum) -> bool {
        // Using the Heretic/Hexen style, no loop

        // let mut cycle_counter = 0;
        // loop {
        if matches!(state, StateNum::S_NULL) {
            self.state = &STATES[StateNum::S_NULL as usize]; //(state_t *)S_NULL;
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
        if let ActionF::Actor(f) = st.action {
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
    pub unsafe fn unset_thing_position(&mut self) {
        if self.flags & MapObjectFlag::NoSector as u32 == 0 {
            (*self.subsector).sector.remove_from_thinglist(self);
        }
    }

    /// P_SetThingPosition, unlink the thing from the sector
    ///
    /// # Safety
    /// Thing must have had a SubSector set on creation.
    pub unsafe fn set_thing_position(&mut self) {
        let level = &mut *self.level;
        let subsector = level.map_data.point_in_subsector_raw(self.xy);
        self.subsector = subsector;

        if self.flags & MapObjectFlag::NoSector as u32 == 0 {
            (*self.subsector).sector.add_to_thinglist(self)
        }
    }

    /// P_RemoveMobj
    pub fn remove(&mut self) {
        // TODO: nightmare respawns
        /*
        if ((mobj->flags & SPECIAL) && !(mobj->flags & DROPPED) &&
            (mobj->type != MT_INV) && (mobj->type != MT_INS)) {
            itemrespawnque[iquehead] = mobj->spawnpoint;
            itemrespawntime[iquehead] = leveltime;
            iquehead = (iquehead + 1) & (ITEMQUESIZE - 1);

            // lose one off the end?
            if (iquehead == iquetail)
            iquetail = (iquetail + 1) & (ITEMQUESIZE - 1);
        }
        */
        unsafe {
            self.unset_thing_position();
        }
        // TODO: S_StopSound(mobj);
        self.thinker_mut().mark_remove();
    }

    /// P_ThingHeightClip
    // Takes a valid thing and adjusts the thing->floorz, thing->ceilingz, and possibly thing->z.
    // This is called for all nearby monsters whenever a sector changes height.
    // If the thing doesn't fit, the z will be set to the lowest value and false will be returned.
    fn height_clip(&mut self) -> bool {
        let on_floor = self.z == self.floorz;

        let mut ctrl = SubSectorMinMax::default();
        self.p_check_position(self.xy, &mut ctrl);
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
    pub fn pit_change_sector(&mut self, no_fit: &mut bool, crush_change: bool) -> bool {
        if self.height_clip() {
            return true;
        }

        if self.health <= 0 {
            self.set_state(StateNum::S_GIBS);

            // TODO: if (gameversion > exe_doom_1_2)
            //  thing->flags &= ~SOLID;

            self.height = 0.0;
            self.radius = 0.0;
            return true;
        }

        // crunch dropped items
        if self.flags & MapObjectFlag::Dropped as u32 != 0 {
            self.remove();
            return true;
        }

        if self.flags & MapObjectFlag::Shootable as u32 == 0 {
            // assume it is bloody gibs or something
            return true;
        }

        *no_fit = true;

        let level_time = unsafe { (*self.level).level_time };

        if crush_change && level_time & 3 == 0 {
            debug!("Crushing!");
            self.p_take_damage(None, None, false, 10);
            let mobj = MapObject::spawn_map_object(
                self.xy.x(),
                self.xy.y(),
                (self.z + self.height) as i32 / 2,
                MapObjectType::MT_BLOOD,
                unsafe { &mut *self.level },
            );
            unsafe {
                (*mobj).momxy.set_x(p_subrandom() as f32 * 0.6); // P_SubRandom() << 12;
                (*mobj).momxy.set_y(p_subrandom() as f32 * 0.6);
            }
        }

        true
    }

    pub fn start_sound(&self, sfx: SfxEnum) {
        unsafe {
            (*self.level).start_sound(
                sfx,
                (self.xy.x(), self.xy.y()),
                self.angle.rad(),
                self as *const Self as usize, // pointer cast as a UID
            )
        }
    }
}

impl Think for MapObject {
    fn think(object: &mut ObjectType, level: &mut Level) -> bool {
        let this = object.mobj();

        if this.momxy.x() != 0.0 || this.momxy.y() != 0.0 || MapObjectFlag::SkullFly as u32 != 0 {
            this.p_xy_movement();

            if this.thinker_mut().should_remove() {
                return true; // mobj was removed
            }
        }

        if (this.z.floor() - this.floorz.floor()).abs() > f32::EPSILON || this.momz != 0.0 {
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
            // check for nightmare respawn
            if this.flags & MapObjectFlag::CountKill as u32 == 0 {
                return false;
            }
            if !level.respawn_monsters {
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
            // TODO: P_NightmareRespawn(mobj);
        }
        false
    }

    fn set_thinker_ptr(&mut self, ptr: *mut Thinker) {
        self.thinker = ptr;
    }

    fn thinker_mut(&mut self) -> &mut Thinker {
        unsafe { &mut *self.thinker }
    }

    fn thinker(&self) -> &Thinker {
        unsafe { &*self.thinker }
    }
}
