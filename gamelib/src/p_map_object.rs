use std::ptr::NonNull;

use crate::level_data::level::Level;
use glam::Vec2;
use wad::lumps::WadThing;

use crate::info::StateNum;
use crate::level_data::map_defs::SubSector;
use crate::{
    angle::Angle,
    d_main::Skill,
    doom_def::{MAXPLAYERS, MTF_AMBUSH, TICRATE},
    info::MapObjectInfo,
    p_local::FRACUNIT_DIV4,
    p_local::ONCEILINGZ,
    DPtr,
};
use crate::{d_thinker::Think, info::map_object_info::MOBJINFO};
use crate::{
    d_thinker::{ActionFunc, Thinker},
    info::states::get_state,
};
use crate::{info::states::State, p_local::p_random, sounds::SfxEnum};
use crate::{
    info::{MapObjectType, SpriteNum},
    p_local::{ONFLOORZ, VIEWHEIGHT},
    player::{Player, PlayerState},
};
use std::f32::consts::PI;
use std::f32::EPSILON;

static MOBJ_CYCLE_LIMIT: u32 = 1000000;
pub static MAXMOVE: f32 = 30.0;
pub static STOPSPEED: f32 = 0.0625; // 0x1000
pub static FRICTION: f32 = 0.90625; // 0xE800

#[derive(Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub enum MapObjectFlag {
    /// Call P_SpecialThing when touched.
    MF_SPECIAL = 1,
    /// Blocks.
    MF_SOLID = 2,
    /// Can be hit.
    MF_SHOOTABLE = 4,
    /// Don't use the sector links (invisible but touchable).
    MF_NOSECTOR = 8,
    /// Don't use the block links (inert but displayable)
    MF_NOBLOCKMAP = 16,

    /// Not to be activated by sound, deaf monster.
    MF_AMBUSH = 32,
    /// Will try to attack right back.
    MF_JUSTHIT = 64,
    /// Will take at least one step before attacking.
    MF_JUSTATTACKED = 128,
    /// On level spawning (initial position),
    ///  hang from ceiling instead of stand on floor.
    MF_SPAWNCEILING = 256,
    /// Don't apply gravity (every tic),
    ///  that is, object will float, keeping current height
    ///  or changing it actively.
    MF_NOGRAVITY = 512,

    /// Movement flags.
    /// This allows jumps from high places.
    MF_DROPOFF = 0x400,
    /// For players, will pick up items.
    MF_PICKUP = 0x800,
    /// Player cheat. ???
    MF_NOCLIP = 0x1000,
    /// Player: keep info about sliding along walls.
    MF_SLIDE = 0x2000,
    /// Allow moves to any height, no gravity.
    /// For active floaters, e.g. cacodemons, pain elementals.
    MF_FLOAT = 0x4000,
    /// Don't cross lines
    ///   ??? or look at heights on teleport.
    MF_TELEPORT = 0x8000,
    /// Don't hit same species, explode on block.
    /// Player missiles as well as fireballs of various kinds.
    MF_MISSILE = 0x10000,
    /// Dropped by a demon, not level spawned.
    /// E.g. ammo clips dropped by dying former humans.
    MF_DROPPED = 0x20000,
    /// Use fuzzy draw (shadow demons or spectres),
    ///  temporary player invisibility powerup.
    MF_SHADOW = 0x40000,
    /// Flag: don't bleed when shot (use puff),
    ///  barrels and shootable furniture shall not bleed.
    MF_NOBLOOD = 0x80000,
    /// Don't stop moving halfway off a step,
    ///  that is, have dead bodies slide down all the way.
    MF_CORPSE = 0x100000,
    /// Floating to a height for a move, ???
    ///  don't auto float to target's height.
    MF_INFLOAT = 0x200000,

    /// On kill, count this enemy object
    ///  towards intermission kill total.
    /// Happy gathering.
    MF_COUNTKILL = 0x400000,

    /// On picking up, count this item object
    ///  towards intermission item total.
    MF_COUNTITEM = 0x800000,

    /// Special handling: skull in flight.
    /// Neither a cacodemon nor a missile.
    MF_SKULLFLY = 0x1000000,

    /// Don't spawn this object
    ///  in death match mode (e.g. key cards).
    MF_NOTDMATCH = 0x2000000,

    /// Player sprites in multiplayer modes are modified
    ///  using an internal color lookup table for re-indexing.
    /// If 0x4 0x8 or 0xc,
    ///  use a translation table for player colormaps
    MF_TRANSLATION = 0xc000000,
    /// Hmm ???.
    MF_TRANSSHIFT = 26,
}

#[derive(Debug)]
pub struct MapObject {
    /// Direct link to the `Thinker` that owns this `MapObject`. Required as
    /// functions on a `MapObject` may need to change the thinker function
    pub thinker: Option<NonNull<Thinker<MapObject>>>,
    /// Info for drawing: position.
    pub xy: Vec2,
    pub z: f32,
    // More list: links in sector (if needed)
    // struct mobj_s*	snext;
    // struct mobj_s*	sprev;
    // Interaction info, by BLOCKMAP.
    // Links in blocks (if needed).
    // struct mobj_s*	bnext;
    // struct mobj_s*	bprev;
    // More drawing info: to determine current sprite.
    /// orientation
    pub angle: Angle,
    /// used to find patch_t and flip value
    sprite: SpriteNum,
    /// might be ORed with FF_FULLBRIGHT
    frame: i32,
    subsector: DPtr<SubSector>,
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
    validcount: i32,
    kind: u16,
    /// &mobjinfo[mobj.type]
    info: MapObjectInfo,
    pub tics: i32,
    /// state tic counter
    // TODO: probably only needs to be an index to the array
    //  using the enum as the indexer
    pub state: State,
    pub flags: u32,
    pub health: i32,
    /// Movement direction, movement generation (zig-zagging).
    /// 0-7
    movedir: i32,
    /// when 0, select a new dir
    movecount: i32,
    // Thing being chased/attacked (or NULL),
    // also the originator for missiles.
    pub target: Option<NonNull<MapObject>>,
    /// Reaction time: if non 0, don't attack yet.
    /// Used by player to freeze a bit after teleporting.
    pub reactiontime: i32,
    /// If >0, the target will be chased
    /// no matter what (even if shot)
    pub threshold: i32,
    /// Additional info record for player avatars only. Only valid if type == MT_PLAYER.
    /// RUST: If this is not `None` then the `NonNull` pointer is guaranteed to point to a player
    pub player: Option<NonNull<Player>>,
    /// Player number last looked for.
    lastlook: i32,
    /// For nightmare respawn.
    spawn_point: Option<WadThing>,
    // Thing being chased/attacked for tracers.
    // struct mobj_s*	tracer;
}

impl Think for MapObject {
    // TODO: P_MobjThinker
    fn think(&mut self, level: &mut Level) -> bool {
        // This is the P_MobjThinker commented out
        // momentum movement
        // if (mobj->momx || mobj->momy || (mobj->flags & MF_SKULLFLY))
        // {
        if self.momxy.x() != 0.0 || self.momxy.y() != 0.0 || MapObjectFlag::MF_SKULLFLY as u32 != 0
        {
            self.p_xy_movement(level);
        }

        if self.was_removed() {
            return true; // mobj was removed
        }

        if (self.z.floor() - self.floorz.floor()).abs() > EPSILON || self.momz != 0.0 {
            self.p_z_movement(level);
            if self.was_removed() {
                return true; // mobj was removed
            }
        }

        // cycle through states,
        // calling action functions at transitions
        if self.tics != -1 {
            self.tics -= 1;

            // you can cycle through multiple states in a tic
            if self.tics > 0 && !self.p_set_mobj_state(self.state.next_state) {
                return true;
            } // freed itself
        } else {
            // check for nightmare respawn
            if self.flags & MapObjectFlag::MF_COUNTKILL as u32 == 0 {
                return false;
            }
            if !level.respawn_monsters {
                return false;
            }
            self.movecount += 1;

            if self.movecount < 12 * TICRATE {
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
}

impl MapObject {
    fn was_removed(&self) -> bool {
        if let Some(thinker) = self.thinker {
            if let ActionFunc::None = unsafe { &thinker.as_ref().function } {
                return true;
            }
        }
        false
    }

    /// P_ExplodeMissile
    pub fn p_explode_missile(&mut self) {
        self.momxy = Vec2::default();
        self.z = 0.0;
        self.p_set_mobj_state(MOBJINFO[self.kind as usize].deathstate);

        self.tics -= (p_random() & 3) as i32;

        if self.tics < 1 {
            self.tics = 1;
        }

        self.flags &= !(MapObjectFlag::MF_MISSILE as u32);

        if self.info.deathsound != SfxEnum::sfx_None {
            // TODO: S_StartSound (mo, mo->info->deathsound);
        }
    }

    /// P_ZMovement
    fn p_z_movement(&mut self, level: &mut Level) {
        if self.player.is_some() && self.z < self.floorz {
            let player = self.player.as_mut().unwrap();
            unsafe {
                let player = player.as_mut();
                player.viewheight -= self.floorz - self.z;
                player.deltaviewheight = (((VIEWHEIGHT - player.viewheight) as i32) >> 3) as f32;
            }
        }

        // adjust height
        self.z += self.momz;

        if self.flags & MapObjectFlag::MF_FLOAT as u32 != 0 && self.target.is_some() {
            // TODO: float down towards target if too close
            // if (!(mo->flags & MF_SKULLFLY) && !(mo->flags & MF_INFLOAT))
            // {
            //     dist = P_AproxDistance(mo->x - mo->target->x,
            //                         mo->y - mo->target->y);

            //     delta = (mo->target->z + (mo->height >> 1)) - mo->z;

            //     if (delta < 0 && dist < -(delta * 3))
            //         mo->z -= FLOATSPEED;
            //     else if (delta > 0 && dist < (delta * 3))
            //         mo->z += FLOATSPEED;
            // }
        }

        // clip movement

        if self.z <= self.floorz {
            // hit the floor
            // TODO: The lost soul correction for old demos
            if self.flags & MapObjectFlag::MF_SKULLFLY as u32 != 0 {
                // the skull slammed into something
                self.momz = -self.momz;
            }

            if self.momz < 0.0 {
                // TODO: is the gravity correct? (FRACUNIT == GRAVITY)
                if self.player.is_some() && self.momz < -1.0 * 8.0 {
                    // Squat down.
                    // Decrease viewheight for a moment
                    // after hitting the ground (hard),
                    // and utter appropriate sound.
                    let player = self.player.as_mut().unwrap();
                    unsafe {
                        player.as_mut().viewheight = ((self.momz as i32) >> 3) as f32;
                    }
                }
                self.momz = 0.0;
            }

            self.z = self.floorz;
        } else if self.flags & MapObjectFlag::MF_NOGRAVITY as u32 == 0 {
            if self.momz == 0.0 {
                self.momz = -1.0 * 2.0;
            } else {
                self.momz -= 1.0;
            }
        }
    }

    /// P_XYMovement
    fn p_xy_movement(&mut self, level: &mut Level) {
        if self.momxy.x() <= 0.0000001
            && self.momxy.x() >= -0.0000001
            && self.momxy.y() <= 0.0000001
            && self.momxy.y() >= -0.0000001
        {
            if self.flags & MapObjectFlag::MF_SKULLFLY as u32 != 0 {
                self.flags &= !(MapObjectFlag::MF_SKULLFLY as u32);
                self.momxy = Vec2::default();
                self.z = 0.0;
                self.p_set_mobj_state(self.info.spawnstate);
            }
            return;
        }

        if self.momxy.x() > MAXMOVE {
            self.momxy.set_x(MAXMOVE);
        } else if self.momxy.x() < -MAXMOVE {
            self.momxy.set_x(-MAXMOVE);
        }

        if self.momxy.y() > MAXMOVE {
            self.momxy.set_y(MAXMOVE);
        } else if self.momxy.y() < -MAXMOVE {
            self.momxy.set_y(-MAXMOVE);
        }

        // This whole loop is a bit crusty. It consists of looping over progressively smaller
        // moves until we either hit 0, or get a move. Because the whole game is 2D we can
        // use modern 2D collision detection where if there is a seg/wall penetration then we
        // move the player back by the penetration amount. This would also make the "slide" stuff
        // a lot easier (but perhaps not as accurate to Doom classic?)
        // Oh yeah, this would also remove:
        //  - linedef BBox,
        //  - BBox checks (these are AABB)
        //  - the need to store line slopes
        // TODO: The above stuff, refactor the collisions and movement to use modern techniques

        // `p_try_move` will apply the move if it is valid, and do specials, explodes etc
        let mut xmove = self.momxy.x();
        let mut ymove = self.momxy.y();
        let mut ptryx;
        let mut ptryy;
        loop {
            if xmove > MAXMOVE / 2.0 || ymove > MAXMOVE / 2.0 {
                ptryx = self.xy.x() + xmove / 2.0;
                ptryy = self.xy.y() + ymove / 2.0;
                xmove /= 2.0;
                ymove /= 2.0;
            } else {
                ptryx = self.xy.x() + xmove;
                ptryy = self.xy.y() + ymove;
                xmove = 0.0;
                ymove = 0.0;
            }

            if !self.p_try_move(ptryx, ptryy, level) {
                if self.player.is_some() {
                    self.p_slide_move(level);
                } else if self.flags & MapObjectFlag::MF_MISSILE as u32 != 0 {
                    // TODO: explode
                } else {
                    self.momxy.set_x(0.0);
                    self.momxy.set_y(0.0);
                }
            }

            if xmove == 0.0 || ymove == 0.0 {
                break;
            }
        }

        // slow down

        if self.flags & (MapObjectFlag::MF_MISSILE as u32 | MapObjectFlag::MF_SKULLFLY as u32) != 0
        {
            return; // no friction for missiles ever
        }

        if self.z > self.floorz {
            return; // no friction when airborne
        }

        if self.flags & MapObjectFlag::MF_CORPSE as u32 != 0 {
            // do not stop sliding
            //  if halfway off a step with some momentum
            if (self.momxy.x() > FRACUNIT_DIV4
                || self.momxy.x() < -FRACUNIT_DIV4
                || self.momxy.y() > FRACUNIT_DIV4
                || self.momxy.y() < -FRACUNIT_DIV4)
                && (self.floorz - self.subsector.sector.floorheight).abs() > EPSILON
            {
                return;
            }
        }

        if self.momxy.x() > -STOPSPEED
            && self.momxy.x() < STOPSPEED
            && self.momxy.y() > -STOPSPEED
            && self.momxy.y() < STOPSPEED
        {
            if self.player.is_none() {
                self.momxy = Vec2::default();
                return;
            } else if let Some(player) = self.player {
                if unsafe {
                    player.as_ref().cmd.forwardmove == 0 && player.as_ref().cmd.sidemove == 0
                } {
                    // if in a walking frame, stop moving
                    // TODO: What the everliving fuck is C doing here? You can't just subtract the states array
                    // if ((player.mo.state - states) - S_PLAY_RUN1) < 4 {
                    //     self.p_set_mobj_state(StateNum::S_PLAY);
                    // }
                    self.momxy = Vec2::default();
                    return;
                }
            }
        } else {
            self.momxy *= FRICTION;
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

        let mut player = &mut players[mthing.kind as usize - 1];

        if player.player_state == PlayerState::PstReborn {
            player.player_reborn();
        }

        // Doom spawns this in it's memory manager then passes a pointer back. As fasr as I can see
        // the Player object owns this.
        let mut thinker = MapObject::p_spawn_map_object(
            mthing.x as f32,
            mthing.y as f32,
            ONFLOORZ,
            MapObjectType::MT_PLAYER as u16,
            level,
        );
        let mut mobj = &mut thinker.obj;

        // set color translations for player sprites
        if mthing.kind > 1 {
            mobj.flags =
                mobj.flags as u32 | (mthing.kind as u32 - 1) << MapObjectFlag::MF_TRANSSHIFT as u8;
        }

        // TODO: check this angle stuff
        mobj.angle = Angle::new(mthing.angle as f32 * PI / 180.0);
        mobj.health = player.health;

        player.mobj = Some(thinker); // TODO: needs to be a pointer to this mapobject in a container which will not move/realloc
        player.player_state = PlayerState::PstLive;
        player.refire = 0;
        player.message = None;
        player.damagecount = 0;
        player.bonuscount = 0;
        player.extralight = 0;
        player.fixedcolormap = 0;
        player.viewheight = VIEWHEIGHT;

        let player_ptr = unsafe { NonNull::new_unchecked(player as *mut Player) };

        if let Some(ref mut think) = player.mobj {
            think.obj.player = Some(player_ptr);
        }

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
        if mthing.kind <= 4 {
            // save spots for respawning in network games
            level.player_starts[(mthing.kind - 1) as usize] = Some(*mthing);
            if !level.deathmatch {
                MapObject::p_spawn_player(mthing, level, players, active_players);
            }
            return;
        }

        // check for apropriate skill level
        let bit: i16;
        if level.game_skill == Skill::Baby {
            bit = 1;
        } else if level.game_skill == Skill::Nightmare {
            bit = 4;
        } else {
            bit = level.game_skill as i16 - 1;
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
            println!(
                "P_SpawnMapThing: Unknown type {} at ({}, {})",
                mthing.kind, mthing.x, mthing.y
            );
        }

        // don't spawn keycards and players in deathmatch
        if level.deathmatch && MOBJINFO[i as usize].flags & MapObjectFlag::MF_NOTDMATCH as u32 != 0
        {
            return;
        }

        // TODO: don't spawn any monsters if -nomonsters
        // if (nomonsters && (i == MT_SKULL || (mobjinfo[i].flags & MF_COUNTKILL)))
        // {
        //     return;
        // }

        let x = mthing.x as f32;
        let y = mthing.y as f32;
        let z;

        if MOBJINFO[i as usize].flags & MapObjectFlag::MF_SPAWNCEILING as u32 != 0 {
            z = ONCEILINGZ;
        } else {
            z = ONFLOORZ;
        }

        let mut thinker = MapObject::p_spawn_map_object(x, y, z, i, level);
        let mobj = &mut thinker.obj;
        if mobj.tics > 0 {
            mobj.tics = 1 + ((p_random() as i32) % mobj.tics);
        }
        if mobj.flags & MapObjectFlag::MF_COUNTKILL as u32 != 0 {
            level.totalkills += 1;
        }
        if mobj.flags & MapObjectFlag::MF_COUNTITEM as u32 != 0 {
            level.totalitems += 1;
        }

        // TODO: check the angle is correct
        mobj.angle = Angle::new(mthing.angle as f32 * PI / 180.0);
        if mthing.flags & MTF_AMBUSH != 0 {
            mobj.flags |= MapObjectFlag::MF_AMBUSH as u32;
        }

        // P_AddThinker(&mobj->thinker);
        if !level.add_thinker(thinker) {
            panic!(
                "P_SpawnMapThing: Could not spawn type {} at ({}, {}): out of memory",
                mthing.kind, mthing.x, mthing.y
            );
        }
    }

    /// P_SpawnMobj
    ///
    /// The callee is expected to handle adding the thinker with P_AddThinker, and
    /// inserting in to the level thinker container (differently to doom).
    ///
    // TODO: pass in a ref to the container so the obj can be added
    //  Doom calls an zmalloc function for this. Then pass a reference back for it
    fn p_spawn_map_object(
        x: f32,
        y: f32,
        mut z: i32,
        kind: u16,
        level: &mut Level,
    ) -> Thinker<MapObject> {
        // // memset(mobj, 0, sizeof(*mobj)); // zeroes out all fields
        let info = MOBJINFO[kind as usize].clone();

        // TODO: Nightmare reactiontimes?
        let reactiontime = if level.game_skill != Skill::Nightmare {
            info.reactiontime
        } else {
            info.reactiontime
        };

        // // do not set the state with P_SetMobjState,
        // // because action routines can not be called yet
        let state = get_state(info.spawnstate as usize);

        // // set subsector and/or block links
        let sub_sector: DPtr<SubSector> = level.map_data.point_in_subsector(Vec2::new(x, y));

        let floorz = sub_sector.sector.floorheight as i32;
        let ceilingz = sub_sector.sector.ceilingheight as i32;

        if z == ONFLOORZ {
            z = floorz;
        } else if z == ONCEILINGZ {
            z = ceilingz - info.height as i32;
        }

        let obj: MapObject = MapObject {
            // The thinker should be non-zero and requires to be added to the linked list
            thinker: None, // TODO: change after thinker container added
            player: None,
            xy: Vec2::new(x, y),
            z: z as f32,
            angle: Angle::new(0.0),
            sprite: state.sprite,
            frame: state.frame,
            floorz: floorz as f32,
            ceilingz: ceilingz as f32,
            radius: info.radius,
            height: info.height,
            momxy: Vec2::default(),
            momz: 0.0,
            validcount: 0,
            flags: info.flags,
            health: info.spawnhealth,
            tics: state.tics,
            movedir: 0,
            movecount: 0,
            reactiontime,
            threshold: 0,
            lastlook: (p_random() as i32) % MAXPLAYERS as i32,
            spawn_point: None,
            target: None,
            subsector: sub_sector,
            state,
            info,
            kind,
        };

        let mut thinker = Thinker::new(obj);
        thinker.function = ActionFunc::None; //P_MobjThinker

        // P_AddThinker(&mobj->thinker);
        thinker
    }

    /// P_SetMobjState
    pub fn p_set_mobj_state(&mut self, mut state: StateNum) -> bool {
        let mut cycle_counter = 0;

        loop {
            match state {
                StateNum::S_NULL => {
                    self.state = get_state(state as usize); //(state_t *)S_NULL;
                                                            //  P_RemoveMobj(mobj);
                    return false;
                }
                _ => {
                    let st = get_state(state as usize);
                    state = st.next_state;

                    // Modified handling.
                    // Call action functions when the state is set
                    let func = self.state.action.mobj_func();
                    unsafe { (*func)(self) }

                    self.tics = st.tics;
                    self.sprite = st.sprite;
                    self.frame = st.frame;
                    self.state = st;
                }
            }

            cycle_counter += 1;
            if cycle_counter > MOBJ_CYCLE_LIMIT {
                println!("P_SetMobjState: Infinite state cycle detected!");
            }

            if self.tics <= 0 {
                break;
            }
        }

        return true;
    }
}
