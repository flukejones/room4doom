use std::{f32::consts::FRAC_PI_4, ptr::NonNull};

use glam::Vec2;
use wad::{
    lumps::{SubSector, Thing},
    DPtr,
};

use crate::info::mapobject::MOBJINFO;
use crate::info::states::{State, STATESJ};
use crate::local::test_action;
use crate::thinker::Thinker;
use crate::{
    angle::Angle, bsp::Bsp, info::MapObjectInfo, local::ONCEILINGZ,
    thinker::ActionF,
};
use crate::{
    info::{MapObjectType, SpriteNum},
    local::{ONFLOORZ, VIEWHEIGHT},
    player::{Player, PlayerState},
};

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum MapObjectFlag {
    /// Call P_SpecialThing when touched.
    MF_SPECIAL      = 1,
    /// Blocks.
    MF_SOLID        = 2,
    /// Can be hit.
    MF_SHOOTABLE    = 4,
    /// Don't use the sector links (invisible but touchable).
    MF_NOSECTOR     = 8,
    /// Don't use the blocklinks (inert but displayable)
    MF_NOBLOCKMAP   = 16,

    /// Not to be activated by sound, deaf monster.
    MF_AMBUSH       = 32,
    /// Will try to attack right back.
    MF_JUSTHIT      = 64,
    /// Will take at least one step before attacking.
    MF_JUSTATTACKED = 128,
    /// On level spawning (initial position),
    ///  hang from ceiling instead of stand on floor.
    MF_SPAWNCEILING = 256,
    /// Don't apply gravity (every tic),
    ///  that is, object will float, keeping current height
    ///  or changing it actively.
    MF_NOGRAVITY    = 512,

    /// Movement flags.
    /// This allows jumps from high places.
    MF_DROPOFF      = 0x400,
    /// For players, will pick up items.
    MF_PICKUP       = 0x800,
    /// Player cheat. ???
    MF_NOCLIP       = 0x1000,
    /// Player: keep info about sliding along walls.
    MF_SLIDE        = 0x2000,
    /// Allow moves to any height, no gravity.
    /// For active floaters, e.g. cacodemons, pain elementals.
    MF_FLOAT        = 0x4000,
    /// Don't cross lines
    ///   ??? or look at heights on teleport.
    MF_TELEPORT     = 0x8000,
    /// Don't hit same species, explode on block.
    /// Player missiles as well as fireballs of various kinds.
    MF_MISSILE      = 0x10000,
    /// Dropped by a demon, not level spawned.
    /// E.g. ammo clips dropped by dying former humans.
    MF_DROPPED      = 0x20000,
    /// Use fuzzy draw (shadow demons or spectres),
    ///  temporary player invisibility powerup.
    MF_SHADOW       = 0x40000,
    /// Flag: don't bleed when shot (use puff),
    ///  barrels and shootable furniture shall not bleed.
    MF_NOBLOOD      = 0x80000,
    /// Don't stop moving halfway off a step,
    ///  that is, have dead bodies slide down all the way.
    MF_CORPSE       = 0x100000,
    /// Floating to a height for a move, ???
    ///  don't auto float to target's height.
    MF_INFLOAT      = 0x200000,

    /// On kill, count this enemy object
    ///  towards intermission kill total.
    /// Happy gathering.
    MF_COUNTKILL    = 0x400000,

    /// On picking up, count this item object
    ///  towards intermission item total.
    MF_COUNTITEM    = 0x800000,

    /// Special handling: skull in flight.
    /// Neither a cacodemon nor a missile.
    MF_SKULLFLY     = 0x1000000,

    /// Don't spawn this object
    ///  in death match mode (e.g. key cards).
    MF_NOTDMATCH    = 0x2000000,

    /// Player sprites in multiplayer modes are modified
    ///  using an internal color lookup table for re-indexing.
    /// If 0x4 0x8 or 0xc,
    ///  use a translation table for player colormaps
    MF_TRANSLATION  = 0xc000000,
    /// Hmm ???.
    MF_TRANSSHIFT   = 26,
}

#[derive(Debug)]
pub struct MapObject<'p> {
    // List: thinker links.
    pub thinker:  Option<NonNull<Thinker<'p>>>,
    /// Info for drawing: position.
    xy:           Vec2,
    z:            f32,
    // More list: links in sector (if needed)
    // struct mobj_s*	snext;
    // struct mobj_s*	sprev;
    // More drawing info: to determine current sprite.
    /// orientation
    angle:        Angle,
    /// used to find patch_t and flip value
    sprite:       SpriteNum,
    /// might be ORed with FF_FULLBRIGHT
    frame:        i32,
    // Interaction info, by BLOCKMAP.
    // Links in blocks (if needed).
    // struct mobj_s*	bnext;
    // struct mobj_s*	bprev;
    sub_sector:   DPtr<SubSector>,
    /// The closest interval over all contacted Sectors.
    floorz:       f32,
    ceilingz:     f32,
    /// For movement checking.
    radius:       f32,
    height:       f32,
    /// Momentums, used to update position.
    momx:         f32,
    momy:         f32,
    momz:         f32,
    /// If == validcount, already checked.
    validcount:   i32,
    kind:         MapObjectType,
    /// &mobjinfo[mobj.type]
    info:         MapObjectInfo,
    tics:         i32,
    /// state tic counter
    // TODO: probably only needs to be an index to the array
    //  using the enum as the indexer
    state:        &'p State,
    flags:        u32,
    health:       i32,
    /// Movement direction, movement generation (zig-zagging).
    /// 0-7
    movedir:      i32,
    /// when 0, select a new dir
    movecount:    i32,
    // Thing being chased/attacked (or NULL),
    // also the originator for missiles.
    // struct mobj_s*	target;
    /// Reaction time: if non 0, don't attack yet.
    /// Used by player to freeze a bit after teleporting.
    reactiontime: i32,
    /// If >0, the target will be chased
    /// no matter what (even if shot)
    threshold:    i32,
    // Additional info record for player avatars only.
    // Only valid if type == MT_PLAYER
    player:       Option<*mut Player<'p>>,
    /// Player number last looked for.
    lastlook:     i32,
    /// For nightmare respawn.
    spawn_point:  Option<Thing>,
    // Thing being chased/attacked for tracers.
    // struct mobj_s*	tracer;
}

impl<'p> MapObject<'p> {
    /// P_SpawnPlayer
    /// Called when a player is spawned on the level.
    /// Most of the player structure stays unchanged
    ///  between levels.
    ///
    /// Called in game.c
    pub fn p_spawn_player<'b>(
        mthing: &Thing,
        bsp: &'b Bsp,
        players: &'b mut [Player<'b>],
    ) {
        // players is a globally accessible thingy
        //p = &players[mthing.type-1];

        if mthing.kind == 0 {
            return;
        }

        // not playing?
        // Network thing
        // if !playeringame[mthing.kind - 1] {
        //     return;
        // }

        let mut p = &mut players[mthing.kind as usize - 1];

        // if p.playerstate == PlayerState::PstReborn {
        //     G_PlayerReborn(mthing.kind - 1);
        // }

        let x = mthing.pos.x();
        let y = mthing.pos.y();
        let z = ONFLOORZ as f32;
        // Doom spawns this in it's memory manager then passes a pointer back. As fasr as I can see
        // the Player object owns this.
        let mut mobj = MapObject::p_spawn_map_object(
            x,
            y,
            z as i32,
            MapObjectType::MT_PLAYER,
            bsp,
        );

        // set color translations for player sprites
        if mthing.kind > 1 {
            mobj.flags = mobj.flags as u32
                | (mthing.kind as u32 - 1)
                    << MapObjectFlag::MF_TRANSSHIFT as u8;
        }

        mobj.angle = Angle::new(FRAC_PI_4 * (mthing.angle / 45.0));
        mobj.health = p.health;

        mobj.player = Some(p as *mut Player); // TODO: needs to be a pointer

        p.mo = Some(mobj); // TODO: needs to be a pointer to this mapobject in a container which will not move/realloc
        p.playerstate = PlayerState::PstLive;
        p.refire = 0;
        p.message = None;
        p.damagecount = 0;
        p.bonuscount = 0;
        p.extralight = 0;
        p.fixedcolormap = 0;
        p.viewheight = VIEWHEIGHT as f32;

        // // setup gun psprite
        // P_SetupPsprites(p);

        // // give all cards in death match mode
        // if deathmatch {
        //     for i in 0..Card::NUMCARDS as usize {
        //         p.cards[i] = true;
        //     }
        // }

        // if mthing.kind - 1 == consoleplayer {
        //     // wake up the status bar
        //     ST_Start();
        //     // wake up the heads up text
        //     HU_Start();
        // }
    }

    /// P_SpawnMobj
    pub fn p_spawn_map_object(
        x: f32,
        y: f32,
        mut z: i32,
        kind: MapObjectType,
        bsp: &Bsp,
    ) -> MapObject {
        // // memset(mobj, 0, sizeof(*mobj)); // zeroes out all fields
        let info = MOBJINFO[kind as usize].clone();

        // if (gameskill != sk_nightmare)
        //     mobj->reactiontime = info->reactiontime;

        // mobj->lastlook = P_Random() % MAXPLAYERS;
        // // do not set the state with P_SetMobjState,
        // // because action routines can not be called yet
        let state: &State = &STATESJ[info.spawnstate as usize];

        // // set subsector and/or block links
        let sub_sector: DPtr<SubSector> =
            bsp.point_in_subsector(&Vec2::new(x, y)).unwrap();

        let floorz = sub_sector.sector.floor_height as i32;
        let ceilingz = sub_sector.sector.ceil_height as i32;

        if z == ONFLOORZ {
            z = floorz;
        } else if z == ONCEILINGZ {
            z = ceilingz - info.height as i32;
        }

        // mobj->thinker.function.acp1 = (actionf_p1)P_MobjThinker;

        // P_AddThinker(&mobj->thinker);

        MapObject {
            thinker: None, // TODO: change after thinker container added
            player: None,
            xy: Vec2::new(x, y),
            z: z as f32,
            angle: Angle::new(0.0),
            sprite: state.sprite,
            frame: state.frame,
            sub_sector: sub_sector,
            floorz: floorz as f32,
            ceilingz: ceilingz as f32,
            radius: info.radius,
            height: info.height,
            momx: 0.0,
            momy: 0.0,
            momz: 0.0,
            validcount: 0,
            flags: info.flags,
            health: info.spawnhealth,
            tics: state.tics,
            // TODO: this may or may not need a clone instead. But because the
            //  containing array is const and there is no `mut` it should be fine
            movedir: 0,
            movecount: 0,
            reactiontime: info.reactiontime,
            threshold: 0,
            lastlook: 2,
            spawn_point: None,
            state,
            info,
            kind,
        }
    }
}
