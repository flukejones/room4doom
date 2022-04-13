use crate::{
    info::{MapObjectInfo, MapObjectType, StateNum},
    play::mobj::MapObjectFlag,
};

use super::SfxEnum;

/// This variable exists only to help create the mobs array
const NUM_CATEGORIES: usize = MapObjectType::NUMMOBJTYPES as usize;

pub const MOBJINFO: [MapObjectInfo; NUM_CATEGORIES] = [
    // MT_PLAYER
    MapObjectInfo::new(
        -1,                     // doomednum
        StateNum::S_PLAY,       // spawnstate
        100,                    // spawnhealth
        StateNum::S_PLAY_RUN1,  // seestate
        SfxEnum::None,          // seesound
        0,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_PLAY_PAIN,  // painstate
        255,                    // painchance
        SfxEnum::plpain,        // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_PLAY_ATK1,  // missilestate
        StateNum::S_PLAY_DIE1,  // deathstate
        StateNum::S_PLAY_XDIE1, // xdeathstate
        SfxEnum::pldeth,        // deathsound
        0.0,                    // speed
        16.0,                   // radius
        56.0,                   // height
        100,                    // mass
        0,                      // damage
        SfxEnum::None,          // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::DropOff as u32
            | MapObjectFlag::Pickup as u32
            | MapObjectFlag::NotDeathmatch as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_POSSESSED
        3004,                   // doomednum
        StateNum::S_POSS_STND,  // spawnstate
        20,                     // spawnhealth
        StateNum::S_POSS_RUN1,  // seestate
        SfxEnum::posit1,        // seesound
        8,                      // reactiontime
        SfxEnum::pistol,        // attacksound
        StateNum::S_POSS_PAIN,  // painstate
        200,                    // painchance
        SfxEnum::popain,        // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_POSS_ATK1,  // missilestate
        StateNum::S_POSS_DIE1,  // deathstate
        StateNum::S_POSS_XDIE1, // xdeathstate
        SfxEnum::podth1,        // deathsound
        8.0,                    // speed
        20.0,                   // radius
        56.0,                   // height
        100,                    // mass
        0,                      // damage
        SfxEnum::posact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_POSS_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_SHOTGUY
        9,                      // doomednum
        StateNum::S_POSS_STND,  // spawnstate
        30,                     // spawnhealth
        StateNum::S_POSS_RUN1,  // seestate
        SfxEnum::posit2,        // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_POSS_PAIN,  // painstate
        170,                    // painchance
        SfxEnum::popain,        // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_POSS_ATK1,  // missilestate
        StateNum::S_POSS_DIE1,  // deathstate
        StateNum::S_POSS_XDIE1, // xdeathstate
        SfxEnum::podth2,        // deathsound
        8.0,                    // speed
        20.0,                   // radius
        56.0,                   // height
        100,                    // mass
        0,                      // damage
        SfxEnum::posact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_POSS_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_VILE
        64,                    // doomednum
        StateNum::S_VILE_STND, // spawnstate
        700,                   // spawnhealth
        StateNum::S_VILE_RUN1, // seestate
        SfxEnum::vilsit,       // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_VILE_PAIN, // painstate
        10,                    // painchance
        SfxEnum::vipain,       // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_VILE_ATK1, // missilestate
        StateNum::S_VILE_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::vildth,       // deathsound
        15.0,                  // speed
        20.0,                  // radius
        56.0,                  // height
        500,                   // mass
        0,                     // damage
        SfxEnum::vilact,       // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_FIRE
        -1,                                                                 // doomednum
        StateNum::S_FIRE1,                                                  // spawnstate
        1000,                                                               // spawnhealth
        StateNum::S_NULL,                                                   // seestate
        SfxEnum::None,                                                      // seesound
        8,                                                                  // reactiontime
        SfxEnum::None,                                                      // attacksound
        StateNum::S_NULL,                                                   // painstate
        0,                                                                  // painchance
        SfxEnum::None,                                                      // painsound
        StateNum::S_NULL,                                                   // meleestate
        StateNum::S_NULL,                                                   // missilestate
        StateNum::S_NULL,                                                   // deathstate
        StateNum::S_NULL,                                                   // xdeathstate
        SfxEnum::None,                                                      // deathsound
        0.0,                                                                // speed
        20.0,                                                               // radius
        16.0,                                                               // height
        100,                                                                // mass
        0,                                                                  // damage
        SfxEnum::None,                                                      // activesound
        MapObjectFlag::NoBlockMap as u32 | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,                                                   // raisestate
    ),
    MapObjectInfo::new(
        // MT_UNDEAD
        66,                     // doomednum
        StateNum::S_SKEL_STND,  // spawnstate
        300,                    // spawnhealth
        StateNum::S_SKEL_RUN1,  // seestate
        SfxEnum::skesit,        // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_SKEL_PAIN,  // painstate
        100,                    // painchance
        SfxEnum::popain,        // painsound
        StateNum::S_SKEL_FIST1, // meleestate
        StateNum::S_SKEL_MISS1, // missilestate
        StateNum::S_SKEL_DIE1,  // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::skedth,        // deathsound
        10.0,                   // speed
        20.0,                   // radius
        56.0,                   // height
        500,                    // mass
        0,                      // damage
        SfxEnum::skeact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_SKEL_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_TRACER
        -1,                    // doomednum
        StateNum::S_TRACER,    // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::skeatk,       // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::None,         // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_TRACEEXP1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::barexp,       // deathsound
        10.0,                  // speed
        11.0,                  // radius
        8.0,                   // height
        100,                   // mass
        10,                    // damage
        SfxEnum::None,         // activesound
        MapObjectFlag::NoBlockMap as u32
            | MapObjectFlag::Missile as u32
            | MapObjectFlag::DropOff as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_SMOKE
        -1,                                                                 // doomednum
        StateNum::S_SMOKE1,                                                 // spawnstate
        1000,                                                               // spawnhealth
        StateNum::S_NULL,                                                   // seestate
        SfxEnum::None,                                                      // seesound
        8,                                                                  // reactiontime
        SfxEnum::None,                                                      // attacksound
        StateNum::S_NULL,                                                   // painstate
        0,                                                                  // painchance
        SfxEnum::None,                                                      // painsound
        StateNum::S_NULL,                                                   // meleestate
        StateNum::S_NULL,                                                   // missilestate
        StateNum::S_NULL,                                                   // deathstate
        StateNum::S_NULL,                                                   // xdeathstate
        SfxEnum::None,                                                      // deathsound
        0.0,                                                                // speed
        20.0,                                                               // radius
        16.0,                                                               // height
        100,                                                                // mass
        0,                                                                  // damage
        SfxEnum::None,                                                      // activesound
        MapObjectFlag::NoBlockMap as u32 | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,                                                   // raisestate
    ),
    MapObjectInfo::new(
        // MT_FATSO
        67,                    // doomednum
        StateNum::S_FATT_STND, // spawnstate
        600,                   // spawnhealth
        StateNum::S_FATT_RUN1, // seestate
        SfxEnum::mansit,       // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_FATT_PAIN, // painstate
        80,                    // painchance
        SfxEnum::mnpain,       // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_FATT_ATK1, // missilestate
        StateNum::S_FATT_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::mandth,       // deathsound
        8.0,                   // speed
        48.0,                  // radius
        64.0,                  // height
        1000,                  // mass
        0,                     // damage
        SfxEnum::posact,       // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_FATT_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_FATSHOT
        -1,                    // doomednum
        StateNum::S_FATSHOT1,  // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::firsht,       // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::None,         // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_FATSHOTX1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::firxpl,       // deathsound
        20.0,                  // speed
        6.0,                   // radius
        8.0,                   // height
        100,                   // mass
        8,                     // damage
        SfxEnum::None,         // activesound
        MapObjectFlag::NoBlockMap as u32
            | MapObjectFlag::Missile as u32
            | MapObjectFlag::DropOff as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_CHAINGUY
        65,                     // doomednum
        StateNum::S_CPOS_STND,  // spawnstate
        70,                     // spawnhealth
        StateNum::S_CPOS_RUN1,  // seestate
        SfxEnum::posit2,        // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_CPOS_PAIN,  // painstate
        170,                    // painchance
        SfxEnum::popain,        // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_CPOS_ATK1,  // missilestate
        StateNum::S_CPOS_DIE1,  // deathstate
        StateNum::S_CPOS_XDIE1, // xdeathstate
        SfxEnum::podth2,        // deathsound
        8.0,                    // speed
        20.0,                   // radius
        56.0,                   // height
        100,                    // mass
        0,                      // damage
        SfxEnum::posact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_CPOS_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_TROOP
        3001,                   // doomednum
        StateNum::S_TROO_STND,  // spawnstate
        60,                     // spawnhealth
        StateNum::S_TROO_RUN1,  // seestate
        SfxEnum::bgsit1,        // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_TROO_PAIN,  // painstate
        200,                    // painchance
        SfxEnum::popain,        // painsound
        StateNum::S_TROO_ATK1,  // meleestate
        StateNum::S_TROO_ATK1,  // missilestate
        StateNum::S_TROO_DIE1,  // deathstate
        StateNum::S_TROO_XDIE1, // xdeathstate
        SfxEnum::bgdth1,        // deathsound
        8.0,                    // speed
        20.0,                   // radius
        56.0,                   // height
        100,                    // mass
        0,                      // damage
        SfxEnum::bgact,         // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_TROO_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_SERGEANT
        3002,                  // doomednum
        StateNum::S_SARG_STND, // spawnstate
        150,                   // spawnhealth
        StateNum::S_SARG_RUN1, // seestate
        SfxEnum::sgtsit,       // seesound
        8,                     // reactiontime
        SfxEnum::sgtatk,       // attacksound
        StateNum::S_SARG_PAIN, // painstate
        180,                   // painchance
        SfxEnum::dmpain,       // painsound
        StateNum::S_SARG_ATK1, // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_SARG_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sgtdth,       // deathsound
        10.0,                  // speed
        30.0,                  // radius
        56.0,                  // height
        400,                   // mass
        0,                     // damage
        SfxEnum::dmact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_SARG_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_SHADOWS
        58,                    // doomednum
        StateNum::S_SARG_STND, // spawnstate
        150,                   // spawnhealth
        StateNum::S_SARG_RUN1, // seestate
        SfxEnum::sgtsit,       // seesound
        8,                     // reactiontime
        SfxEnum::sgtatk,       // attacksound
        StateNum::S_SARG_PAIN, // painstate
        180,                   // painchance
        SfxEnum::dmpain,       // painsound
        StateNum::S_SARG_ATK1, // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_SARG_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sgtdth,       // deathsound
        10.0,                  // speed
        30.0,                  // radius
        56.0,                  // height
        400,                   // mass
        0,                     // damage
        SfxEnum::dmact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::Shadow as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_SARG_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_HEAD
        3005,                  // doomednum
        StateNum::S_HEAD_STND, // spawnstate
        400,                   // spawnhealth
        StateNum::S_HEAD_RUN1, // seestate
        SfxEnum::cacsit,       // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_HEAD_PAIN, // painstate
        128,                   // painchance
        SfxEnum::dmpain,       // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_HEAD_ATK1, // missilestate
        StateNum::S_HEAD_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::cacdth,       // deathsound
        8.0,                   // speed
        31.0,                  // radius
        56.0,                  // height
        400,                   // mass
        0,                     // damage
        SfxEnum::dmact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::Float as u32
            | MapObjectFlag::NoGravity as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_HEAD_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_BRUISER
        3003,                  // doomednum
        StateNum::S_BOSS_STND, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_BOSS_RUN1, // seestate
        SfxEnum::brssit,       // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_BOSS_PAIN, // painstate
        50,                    // painchance
        SfxEnum::dmpain,       // painsound
        StateNum::S_BOSS_ATK1, // meleestate
        StateNum::S_BOSS_ATK1, // missilestate
        StateNum::S_BOSS_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::brsdth,       // deathsound
        8.0,                   // speed
        24.0,                  // radius
        64.0,                  // height
        1000,                  // mass
        0,                     // damage
        SfxEnum::dmact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_BOSS_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_BRUISERSHOT
        -1,                   // doomednum
        StateNum::S_BRBALL1,  // spawnstate
        1000,                 // spawnhealth
        StateNum::S_NULL,     // seestate
        SfxEnum::firsht,      // seesound
        8,                    // reactiontime
        SfxEnum::None,        // attacksound
        StateNum::S_NULL,     // painstate
        0,                    // painchance
        SfxEnum::None,        // painsound
        StateNum::S_NULL,     // meleestate
        StateNum::S_NULL,     // missilestate
        StateNum::S_BRBALLX1, // deathstate
        StateNum::S_NULL,     // xdeathstate
        SfxEnum::firxpl,      // deathsound
        15.0,                 // speed
        6.0,                  // radius
        8.0,                  // height
        100,                  // mass
        8,                    // damage
        SfxEnum::None,        // activesound
        MapObjectFlag::NoBlockMap as u32
            | MapObjectFlag::Missile as u32
            | MapObjectFlag::DropOff as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,     // raisestate
    ),
    MapObjectInfo::new(
        // MT_KNIGHT
        69,                    // doomednum
        StateNum::S_BOS2_STND, // spawnstate
        500,                   // spawnhealth
        StateNum::S_BOS2_RUN1, // seestate
        SfxEnum::kntsit,       // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_BOS2_PAIN, // painstate
        50,                    // painchance
        SfxEnum::dmpain,       // painsound
        StateNum::S_BOS2_ATK1, // meleestate
        StateNum::S_BOS2_ATK1, // missilestate
        StateNum::S_BOS2_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::kntdth,       // deathsound
        8.0,                   // speed
        24.0,                  // radius
        64.0,                  // height
        1000,                  // mass
        0,                     // damage
        SfxEnum::dmact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_BOS2_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_SKULL
        3006,                   // doomednum
        StateNum::S_SKULL_STND, // spawnstate
        100,                    // spawnhealth
        StateNum::S_SKULL_RUN1, // seestate
        SfxEnum::None,          // seesound
        8,                      // reactiontime
        SfxEnum::sklatk,        // attacksound
        StateNum::S_SKULL_PAIN, // painstate
        256,                    // painchance
        SfxEnum::dmpain,        // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_SKULL_ATK1, // missilestate
        StateNum::S_SKULL_DIE1, // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::firxpl,        // deathsound
        8.0,                    // speed
        16.0,                   // radius
        56.0,                   // height
        50,                     // mass
        3,                      // damage
        SfxEnum::dmact,         // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::Float as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_SPIDER
        7,                     // doomednum
        StateNum::S_SPID_STND, // spawnstate
        3000,                  // spawnhealth
        StateNum::S_SPID_RUN1, // seestate
        SfxEnum::spisit,       // seesound
        8,                     // reactiontime
        SfxEnum::shotgn,       // attacksound
        StateNum::S_SPID_PAIN, // painstate
        40,                    // painchance
        SfxEnum::dmpain,       // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_SPID_ATK1, // missilestate
        StateNum::S_SPID_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::spidth,       // deathsound
        12.0,                  // speed
        128.0,                 // radius
        100.0,                 // height
        1000,                  // mass
        0,                     // damage
        SfxEnum::dmact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_BABY
        68,                     // doomednum
        StateNum::S_BSPI_STND,  // spawnstate
        500,                    // spawnhealth
        StateNum::S_BSPI_SIGHT, // seestate
        SfxEnum::bspsit,        // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_BSPI_PAIN,  // painstate
        128,                    // painchance
        SfxEnum::dmpain,        // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_BSPI_ATK1,  // missilestate
        StateNum::S_BSPI_DIE1,  // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::bspdth,        // deathsound
        12.0,                   // speed
        64.0,                   // radius
        64.0,                   // height
        600,                    // mass
        0,                      // damage
        SfxEnum::bspact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_BSPI_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_CYBORG
        16,                     // doomednum
        StateNum::S_CYBER_STND, // spawnstate
        4000,                   // spawnhealth
        StateNum::S_CYBER_RUN1, // seestate
        SfxEnum::cybsit,        // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_CYBER_PAIN, // painstate
        20,                     // painchance
        SfxEnum::dmpain,        // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_CYBER_ATK1, // missilestate
        StateNum::S_CYBER_DIE1, // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::cybdth,        // deathsound
        16.0,                   // speed
        40.0,                   // radius
        110.0,                  // height
        1000,                   // mass
        0,                      // damage
        SfxEnum::dmact,         // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_PAIN
        71,                    // doomednum
        StateNum::S_PAIN_STND, // spawnstate
        400,                   // spawnhealth
        StateNum::S_PAIN_RUN1, // seestate
        SfxEnum::pesit,        // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_PAIN_PAIN, // painstate
        128,                   // painchance
        SfxEnum::pepain,       // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_PAIN_ATK1, // missilestate
        StateNum::S_PAIN_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::pedth,        // deathsound
        8.0,                   // speed
        31.0,                  // radius
        56.0,                  // height
        400,                   // mass
        0,                     // damage
        SfxEnum::dmact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::Float as u32
            | MapObjectFlag::NoGravity as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_PAIN_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_WOLFSS
        84,                     // doomednum
        StateNum::S_SSWV_STND,  // spawnstate
        50,                     // spawnhealth
        StateNum::S_SSWV_RUN1,  // seestate
        SfxEnum::sssit,         // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_SSWV_PAIN,  // painstate
        170,                    // painchance
        SfxEnum::popain,        // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_SSWV_ATK1,  // missilestate
        StateNum::S_SSWV_DIE1,  // deathstate
        StateNum::S_SSWV_XDIE1, // xdeathstate
        SfxEnum::ssdth,         // deathsound
        8.0,                    // speed
        20.0,                   // radius
        56.0,                   // height
        100,                    // mass
        0,                      // damage
        SfxEnum::posact,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_SSWV_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_KEEN
        72,                   // doomednum
        StateNum::S_KEENSTND, // spawnstate
        100,                  // spawnhealth
        StateNum::S_NULL,     // seestate
        SfxEnum::None,        // seesound
        8,                    // reactiontime
        SfxEnum::None,        // attacksound
        StateNum::S_KEENPAIN, // painstate
        256,                  // painchance
        SfxEnum::keenpn,      // painsound
        StateNum::S_NULL,     // meleestate
        StateNum::S_NULL,     // missilestate
        StateNum::S_COMMKEEN, // deathstate
        StateNum::S_NULL,     // xdeathstate
        SfxEnum::keendt,      // deathsound
        0.0,                  // speed
        16.0,                 // radius
        72.0,                 // height
        10000000,             // mass
        0,                    // damage
        SfxEnum::None,        // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::SpawnCeiling as u32
            | MapObjectFlag::NoGravity as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::CountKill as u32, // flags
        StateNum::S_NULL,     // raisestate
    ),
    MapObjectInfo::new(
        // MT_BOSSBRAIN
        88,                                                            // doomednum
        StateNum::S_BRAIN,                                             // spawnstate
        250,                                                           // spawnhealth
        StateNum::S_NULL,                                              // seestate
        SfxEnum::None,                                                 // seesound
        8,                                                             // reactiontime
        SfxEnum::None,                                                 // attacksound
        StateNum::S_BRAIN_PAIN,                                        // painstate
        255,                                                           // painchance
        SfxEnum::bospn,                                                // painsound
        StateNum::S_NULL,                                              // meleestate
        StateNum::S_NULL,                                              // missilestate
        StateNum::S_BRAIN_DIE1,                                        // deathstate
        StateNum::S_NULL,                                              // xdeathstate
        SfxEnum::bosdth,                                               // deathsound
        0.0,                                                           // speed
        16.0,                                                          // radius
        16.0,                                                          // height
        10000000,                                                      // mass
        0,                                                             // damage
        SfxEnum::None,                                                 // activesound
        MapObjectFlag::Solid as u32 | MapObjectFlag::Shootable as u32, // flags
        StateNum::S_NULL,                                              // raisestate
    ),
    MapObjectInfo::new(
        // MT_BOSSSPIT
        89,                                                                // doomednum
        StateNum::S_BRAINEYE,                                              // spawnstate
        1000,                                                              // spawnhealth
        StateNum::S_BRAINEYESEE,                                           // seestate
        SfxEnum::None,                                                     // seesound
        8,                                                                 // reactiontime
        SfxEnum::None,                                                     // attacksound
        StateNum::S_NULL,                                                  // painstate
        0,                                                                 // painchance
        SfxEnum::None,                                                     // painsound
        StateNum::S_NULL,                                                  // meleestate
        StateNum::S_NULL,                                                  // missilestate
        StateNum::S_NULL,                                                  // deathstate
        StateNum::S_NULL,                                                  // xdeathstate
        SfxEnum::None,                                                     // deathsound
        0.0,                                                               // speed
        20.0,                                                              // radius
        32.0,                                                              // height
        100,                                                               // mass
        0,                                                                 // damage
        SfxEnum::None,                                                     // activesound
        MapObjectFlag::NoBlockMap as u32 | MapObjectFlag::NoSector as u32, // flags
        StateNum::S_NULL,                                                  // raisestate
    ),
    MapObjectInfo::new(
        // MT_BOSSTARGET
        87,                                                                // doomednum
        StateNum::S_NULL,                                                  // spawnstate
        1000,                                                              // spawnhealth
        StateNum::S_NULL,                                                  // seestate
        SfxEnum::None,                                                     // seesound
        8,                                                                 // reactiontime
        SfxEnum::None,                                                     // attacksound
        StateNum::S_NULL,                                                  // painstate
        0,                                                                 // painchance
        SfxEnum::None,                                                     // painsound
        StateNum::S_NULL,                                                  // meleestate
        StateNum::S_NULL,                                                  // missilestate
        StateNum::S_NULL,                                                  // deathstate
        StateNum::S_NULL,                                                  // xdeathstate
        SfxEnum::None,                                                     // deathsound
        0.0,                                                               // speed
        20.0,                                                              // radius
        32.0,                                                              // height
        100,                                                               // mass
        0,                                                                 // damage
        SfxEnum::None,                                                     // activesound
        MapObjectFlag::NoBlockMap as u32 | MapObjectFlag::NoSector as u32, // flags
        StateNum::S_NULL,                                                  // raisestate
    ),
    MapObjectInfo::new(
        // MT_SPAWNSHOT
        -1,                 // doomednum
        StateNum::S_SPAWN1, // spawnstate
        1000,               // spawnhealth
        StateNum::S_NULL,   // seestate
        SfxEnum::bospit,    // seesound
        8,                  // reactiontime
        SfxEnum::None,      // attacksound
        StateNum::S_NULL,   // painstate
        0,                  // painchance
        SfxEnum::None,      // painsound
        StateNum::S_NULL,   // meleestate
        StateNum::S_NULL,   // missilestate
        StateNum::S_NULL,   // deathstate
        StateNum::S_NULL,   // xdeathstate
        SfxEnum::firxpl,    // deathsound
        10.0,               // speed
        6.0,                // radius
        32.0,               // height
        100,                // mass
        3,                  // damage
        SfxEnum::None,      // activesound
        MapObjectFlag::NoBlockMap as u32
            | MapObjectFlag::Missile as u32
            | MapObjectFlag::DropOff as u32
            | MapObjectFlag::NoGravity as u32
            | MapObjectFlag::NoClip as u32, // flags
        StateNum::S_NULL,   // raisestate
    ),
    MapObjectInfo::new(
        // MT_SPAWNFIRE
        -1,                                                                 // doomednum
        StateNum::S_SPAWNFIRE1,                                             // spawnstate
        1000,                                                               // spawnhealth
        StateNum::S_NULL,                                                   // seestate
        SfxEnum::None,                                                      // seesound
        8,                                                                  // reactiontime
        SfxEnum::None,                                                      // attacksound
        StateNum::S_NULL,                                                   // painstate
        0,                                                                  // painchance
        SfxEnum::None,                                                      // painsound
        StateNum::S_NULL,                                                   // meleestate
        StateNum::S_NULL,                                                   // missilestate
        StateNum::S_NULL,                                                   // deathstate
        StateNum::S_NULL,                                                   // xdeathstate
        SfxEnum::None,                                                      // deathsound
        0.0,                                                                // speed
        20.0,                                                               // radius
        16.0,                                                               // height
        100,                                                                // mass
        0,                                                                  // damage
        SfxEnum::None,                                                      // activesound
        MapObjectFlag::NoBlockMap as u32 | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,                                                   // raisestate
    ),
    MapObjectInfo::new(
        // MT_BARREL
        2035,             // doomednum
        StateNum::S_BAR1, // spawnstate
        20,               // spawnhealth
        StateNum::S_NULL, // seestate
        SfxEnum::None,    // seesound
        8,                // reactiontime
        SfxEnum::None,    // attacksound
        StateNum::S_NULL, // painstate
        0,                // painchance
        SfxEnum::None,    // painsound
        StateNum::S_NULL, // meleestate
        StateNum::S_NULL, // missilestate
        StateNum::S_BEXP, // deathstate
        StateNum::S_NULL, // xdeathstate
        SfxEnum::barexp,  // deathsound
        0.0,              // speed
        10.0,             // radius
        42.0,             // height
        100,              // mass
        0,                // damage
        SfxEnum::None,    // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::Shootable as u32
            | MapObjectFlag::NoBlood as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_TROOPSHOT
        -1,                  // doomednum
        StateNum::S_TBALL1,  // spawnstate
        1000,                // spawnhealth
        StateNum::S_NULL,    // seestate
        SfxEnum::firsht,     // seesound
        8,                   // reactiontime
        SfxEnum::None,       // attacksound
        StateNum::S_NULL,    // painstate
        0,                   // painchance
        SfxEnum::None,       // painsound
        StateNum::S_NULL,    // meleestate
        StateNum::S_NULL,    // missilestate
        StateNum::S_TBALLX1, // deathstate
        StateNum::S_NULL,    // xdeathstate
        SfxEnum::firxpl,     // deathsound
        10.0,                // speed
        6.0,                 // radius
        8.0,                 // height
        100,                 // mass
        3,                   // damage
        SfxEnum::None,       // activesound
        MapObjectFlag::NoBlockMap as u32
            | MapObjectFlag::Missile as u32
            | MapObjectFlag::DropOff as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,    // raisestate
    ),
    MapObjectInfo::new(
        // MT_HEADSHOT
        -1,                  // doomednum
        StateNum::S_RBALL1,  // spawnstate
        1000,                // spawnhealth
        StateNum::S_NULL,    // seestate
        SfxEnum::firsht,     // seesound
        8,                   // reactiontime
        SfxEnum::None,       // attacksound
        StateNum::S_NULL,    // painstate
        0,                   // painchance
        SfxEnum::None,       // painsound
        StateNum::S_NULL,    // meleestate
        StateNum::S_NULL,    // missilestate
        StateNum::S_RBALLX1, // deathstate
        StateNum::S_NULL,    // xdeathstate
        SfxEnum::firxpl,     // deathsound
        10.0,                // speed
        6.0,                 // radius
        8.0,                 // height
        100,                 // mass
        5,                   // damage
        SfxEnum::None,       // activesound
        MapObjectFlag::NoBlockMap as u32
            | MapObjectFlag::Missile as u32
            | MapObjectFlag::DropOff as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,    // raisestate
    ),
    MapObjectInfo::new(
        // MT_ROCKET
        -1,                   // doomednum
        StateNum::S_ROCKET,   // spawnstate
        1000,                 // spawnhealth
        StateNum::S_NULL,     // seestate
        SfxEnum::rlaunc,      // seesound
        8,                    // reactiontime
        SfxEnum::None,        // attacksound
        StateNum::S_NULL,     // painstate
        0,                    // painchance
        SfxEnum::None,        // painsound
        StateNum::S_NULL,     // meleestate
        StateNum::S_NULL,     // missilestate
        StateNum::S_EXPLODE1, // deathstate
        StateNum::S_NULL,     // xdeathstate
        SfxEnum::barexp,      // deathsound
        20.0,                 // speed
        11.0,                 // radius
        8.0,                  // height
        100,                  // mass
        20,                   // damage
        SfxEnum::None,        // activesound
        MapObjectFlag::NoBlockMap as u32
            | MapObjectFlag::Missile as u32
            | MapObjectFlag::DropOff as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,     // raisestate
    ),
    MapObjectInfo::new(
        // MT_PLASMA
        -1,                   // doomednum
        StateNum::S_PLASBALL, // spawnstate
        1000,                 // spawnhealth
        StateNum::S_NULL,     // seestate
        SfxEnum::plasma,      // seesound
        8,                    // reactiontime
        SfxEnum::None,        // attacksound
        StateNum::S_NULL,     // painstate
        0,                    // painchance
        SfxEnum::None,        // painsound
        StateNum::S_NULL,     // meleestate
        StateNum::S_NULL,     // missilestate
        StateNum::S_PLASEXP,  // deathstate
        StateNum::S_NULL,     // xdeathstate
        SfxEnum::firxpl,      // deathsound
        25.0,                 // speed
        13.0,                 // radius
        8.0,                  // height
        100,                  // mass
        5,                    // damage
        SfxEnum::None,        // activesound
        MapObjectFlag::NoBlockMap as u32
            | MapObjectFlag::Missile as u32
            | MapObjectFlag::DropOff as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,     // raisestate
    ),
    MapObjectInfo::new(
        // MT_BFG
        -1,                  // doomednum
        StateNum::S_BFGSHOT, // spawnstate
        1000,                // spawnhealth
        StateNum::S_NULL,    // seestate
        SfxEnum::None,       // seesound
        8,                   // reactiontime
        SfxEnum::None,       // attacksound
        StateNum::S_NULL,    // painstate
        0,                   // painchance
        SfxEnum::None,       // painsound
        StateNum::S_NULL,    // meleestate
        StateNum::S_NULL,    // missilestate
        StateNum::S_BFGLAND, // deathstate
        StateNum::S_NULL,    // xdeathstate
        SfxEnum::rxplod,     // deathsound
        25.0,                // speed
        13.0,                // radius
        8.0,                 // height
        100,                 // mass
        100,                 // damage
        SfxEnum::None,       // activesound
        MapObjectFlag::NoBlockMap as u32
            | MapObjectFlag::Missile as u32
            | MapObjectFlag::DropOff as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,    // raisestate
    ),
    MapObjectInfo::new(
        // MT_ARACHPLAZ
        -1,                     // doomednum
        StateNum::S_ARACH_PLAZ, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::plasma,        // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::None,          // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_ARACH_PLEX, // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::firxpl,        // deathsound
        25.0,                   // speed
        13.0,                   // radius
        8.0,                    // height
        100,                    // mass
        5,                      // damage
        SfxEnum::None,          // activesound
        MapObjectFlag::NoBlockMap as u32
            | MapObjectFlag::Missile as u32
            | MapObjectFlag::DropOff as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_PUFF
        -1,                                                                 // doomednum
        StateNum::S_PUFF1,                                                  // spawnstate
        1000,                                                               // spawnhealth
        StateNum::S_NULL,                                                   // seestate
        SfxEnum::None,                                                      // seesound
        8,                                                                  // reactiontime
        SfxEnum::None,                                                      // attacksound
        StateNum::S_NULL,                                                   // painstate
        0,                                                                  // painchance
        SfxEnum::None,                                                      // painsound
        StateNum::S_NULL,                                                   // meleestate
        StateNum::S_NULL,                                                   // missilestate
        StateNum::S_NULL,                                                   // deathstate
        StateNum::S_NULL,                                                   // xdeathstate
        SfxEnum::None,                                                      // deathsound
        0.0,                                                                // speed
        20.0,                                                               // radius
        16.0,                                                               // height
        100,                                                                // mass
        0,                                                                  // damage
        SfxEnum::None,                                                      // activesound
        MapObjectFlag::NoBlockMap as u32 | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,                                                   // raisestate
    ),
    MapObjectInfo::new(
        // MT_BLOOD
        -1,                               // doomednum
        StateNum::S_BLOOD1,               // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::None,                    // seesound
        8,                                // reactiontime
        SfxEnum::None,                    // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::None,                    // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::None,                    // deathsound
        0.0,                              // speed
        20.0,                             // radius
        16.0,                             // height
        100,                              // mass
        0,                                // damage
        SfxEnum::None,                    // activesound
        MapObjectFlag::NoBlockMap as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_TFOG
        -1,                                                                 // doomednum
        StateNum::S_TFOG,                                                   // spawnstate
        1000,                                                               // spawnhealth
        StateNum::S_NULL,                                                   // seestate
        SfxEnum::None,                                                      // seesound
        8,                                                                  // reactiontime
        SfxEnum::None,                                                      // attacksound
        StateNum::S_NULL,                                                   // painstate
        0,                                                                  // painchance
        SfxEnum::None,                                                      // painsound
        StateNum::S_NULL,                                                   // meleestate
        StateNum::S_NULL,                                                   // missilestate
        StateNum::S_NULL,                                                   // deathstate
        StateNum::S_NULL,                                                   // xdeathstate
        SfxEnum::None,                                                      // deathsound
        0.0,                                                                // speed
        20.0,                                                               // radius
        16.0,                                                               // height
        100,                                                                // mass
        0,                                                                  // damage
        SfxEnum::None,                                                      // activesound
        MapObjectFlag::NoBlockMap as u32 | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,                                                   // raisestate
    ),
    MapObjectInfo::new(
        // MT_IFOG
        -1,                                                                 // doomednum
        StateNum::S_IFOG,                                                   // spawnstate
        1000,                                                               // spawnhealth
        StateNum::S_NULL,                                                   // seestate
        SfxEnum::None,                                                      // seesound
        8,                                                                  // reactiontime
        SfxEnum::None,                                                      // attacksound
        StateNum::S_NULL,                                                   // painstate
        0,                                                                  // painchance
        SfxEnum::None,                                                      // painsound
        StateNum::S_NULL,                                                   // meleestate
        StateNum::S_NULL,                                                   // missilestate
        StateNum::S_NULL,                                                   // deathstate
        StateNum::S_NULL,                                                   // xdeathstate
        SfxEnum::None,                                                      // deathsound
        0.0,                                                                // speed
        20.0,                                                               // radius
        16.0,                                                               // height
        100,                                                                // mass
        0,                                                                  // damage
        SfxEnum::None,                                                      // activesound
        MapObjectFlag::NoBlockMap as u32 | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,                                                   // raisestate
    ),
    MapObjectInfo::new(
        // MT_TELEPORTMAN
        14,                                                                // doomednum
        StateNum::S_NULL,                                                  // spawnstate
        1000,                                                              // spawnhealth
        StateNum::S_NULL,                                                  // seestate
        SfxEnum::None,                                                     // seesound
        8,                                                                 // reactiontime
        SfxEnum::None,                                                     // attacksound
        StateNum::S_NULL,                                                  // painstate
        0,                                                                 // painchance
        SfxEnum::None,                                                     // painsound
        StateNum::S_NULL,                                                  // meleestate
        StateNum::S_NULL,                                                  // missilestate
        StateNum::S_NULL,                                                  // deathstate
        StateNum::S_NULL,                                                  // xdeathstate
        SfxEnum::None,                                                     // deathsound
        0.0,                                                               // speed
        20.0,                                                              // radius
        16.0,                                                              // height
        100,                                                               // mass
        0,                                                                 // damage
        SfxEnum::None,                                                     // activesound
        MapObjectFlag::NoBlockMap as u32 | MapObjectFlag::NoSector as u32, // flags
        StateNum::S_NULL,                                                  // raisestate
    ),
    MapObjectInfo::new(
        // MT_EXTRABFG
        -1,                                                                 // doomednum
        StateNum::S_BFGEXP,                                                 // spawnstate
        1000,                                                               // spawnhealth
        StateNum::S_NULL,                                                   // seestate
        SfxEnum::None,                                                      // seesound
        8,                                                                  // reactiontime
        SfxEnum::None,                                                      // attacksound
        StateNum::S_NULL,                                                   // painstate
        0,                                                                  // painchance
        SfxEnum::None,                                                      // painsound
        StateNum::S_NULL,                                                   // meleestate
        StateNum::S_NULL,                                                   // missilestate
        StateNum::S_NULL,                                                   // deathstate
        StateNum::S_NULL,                                                   // xdeathstate
        SfxEnum::None,                                                      // deathsound
        0.0,                                                                // speed
        20.0,                                                               // radius
        16.0,                                                               // height
        100,                                                                // mass
        0,                                                                  // damage
        SfxEnum::None,                                                      // activesound
        MapObjectFlag::NoBlockMap as u32 | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,                                                   // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC0
        2018,                          // doomednum
        StateNum::S_ARM1,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC1
        2019,                          // doomednum
        StateNum::S_ARM2,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC2
        2014,                                                            // doomednum
        StateNum::S_BON1,                                                // spawnstate
        1000,                                                            // spawnhealth
        StateNum::S_NULL,                                                // seestate
        SfxEnum::None,                                                   // seesound
        8,                                                               // reactiontime
        SfxEnum::None,                                                   // attacksound
        StateNum::S_NULL,                                                // painstate
        0,                                                               // painchance
        SfxEnum::None,                                                   // painsound
        StateNum::S_NULL,                                                // meleestate
        StateNum::S_NULL,                                                // missilestate
        StateNum::S_NULL,                                                // deathstate
        StateNum::S_NULL,                                                // xdeathstate
        SfxEnum::None,                                                   // deathsound
        0.0,                                                             // speed
        20.0,                                                            // radius
        16.0,                                                            // height
        100,                                                             // mass
        0,                                                               // damage
        SfxEnum::None,                                                   // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::CountItem as u32, // flags
        StateNum::S_NULL,                                                // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC3
        2015,                                                            // doomednum
        StateNum::S_BON2,                                                // spawnstate
        1000,                                                            // spawnhealth
        StateNum::S_NULL,                                                // seestate
        SfxEnum::None,                                                   // seesound
        8,                                                               // reactiontime
        SfxEnum::None,                                                   // attacksound
        StateNum::S_NULL,                                                // painstate
        0,                                                               // painchance
        SfxEnum::None,                                                   // painsound
        StateNum::S_NULL,                                                // meleestate
        StateNum::S_NULL,                                                // missilestate
        StateNum::S_NULL,                                                // deathstate
        StateNum::S_NULL,                                                // xdeathstate
        SfxEnum::None,                                                   // deathsound
        0.0,                                                             // speed
        20.0,                                                            // radius
        16.0,                                                            // height
        100,                                                             // mass
        0,                                                               // damage
        SfxEnum::None,                                                   // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::CountItem as u32, // flags
        StateNum::S_NULL,                                                // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC4
        5,                                                                   // doomednum
        StateNum::S_BKEY,                                                    // spawnstate
        1000,                                                                // spawnhealth
        StateNum::S_NULL,                                                    // seestate
        SfxEnum::None,                                                       // seesound
        8,                                                                   // reactiontime
        SfxEnum::None,                                                       // attacksound
        StateNum::S_NULL,                                                    // painstate
        0,                                                                   // painchance
        SfxEnum::None,                                                       // painsound
        StateNum::S_NULL,                                                    // meleestate
        StateNum::S_NULL,                                                    // missilestate
        StateNum::S_NULL,                                                    // deathstate
        StateNum::S_NULL,                                                    // xdeathstate
        SfxEnum::None,                                                       // deathsound
        0.0,                                                                 // speed
        20.0,                                                                // radius
        16.0,                                                                // height
        100,                                                                 // mass
        0,                                                                   // damage
        SfxEnum::None,                                                       // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::NotDeathmatch as u32, // flags
        StateNum::S_NULL,                                                    // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC5
        13,                                                                  // doomednum
        StateNum::S_RKEY,                                                    // spawnstate
        1000,                                                                // spawnhealth
        StateNum::S_NULL,                                                    // seestate
        SfxEnum::None,                                                       // seesound
        8,                                                                   // reactiontime
        SfxEnum::None,                                                       // attacksound
        StateNum::S_NULL,                                                    // painstate
        0,                                                                   // painchance
        SfxEnum::None,                                                       // painsound
        StateNum::S_NULL,                                                    // meleestate
        StateNum::S_NULL,                                                    // missilestate
        StateNum::S_NULL,                                                    // deathstate
        StateNum::S_NULL,                                                    // xdeathstate
        SfxEnum::None,                                                       // deathsound
        0.0,                                                                 // speed
        20.0,                                                                // radius
        16.0,                                                                // height
        100,                                                                 // mass
        0,                                                                   // damage
        SfxEnum::None,                                                       // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::NotDeathmatch as u32, // flags
        StateNum::S_NULL,                                                    // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC6
        6,                                                                   // doomednum
        StateNum::S_YKEY,                                                    // spawnstate
        1000,                                                                // spawnhealth
        StateNum::S_NULL,                                                    // seestate
        SfxEnum::None,                                                       // seesound
        8,                                                                   // reactiontime
        SfxEnum::None,                                                       // attacksound
        StateNum::S_NULL,                                                    // painstate
        0,                                                                   // painchance
        SfxEnum::None,                                                       // painsound
        StateNum::S_NULL,                                                    // meleestate
        StateNum::S_NULL,                                                    // missilestate
        StateNum::S_NULL,                                                    // deathstate
        StateNum::S_NULL,                                                    // xdeathstate
        SfxEnum::None,                                                       // deathsound
        0.0,                                                                 // speed
        20.0,                                                                // radius
        16.0,                                                                // height
        100,                                                                 // mass
        0,                                                                   // damage
        SfxEnum::None,                                                       // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::NotDeathmatch as u32, // flags
        StateNum::S_NULL,                                                    // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC7
        39,                                                                  // doomednum
        StateNum::S_YSKULL,                                                  // spawnstate
        1000,                                                                // spawnhealth
        StateNum::S_NULL,                                                    // seestate
        SfxEnum::None,                                                       // seesound
        8,                                                                   // reactiontime
        SfxEnum::None,                                                       // attacksound
        StateNum::S_NULL,                                                    // painstate
        0,                                                                   // painchance
        SfxEnum::None,                                                       // painsound
        StateNum::S_NULL,                                                    // meleestate
        StateNum::S_NULL,                                                    // missilestate
        StateNum::S_NULL,                                                    // deathstate
        StateNum::S_NULL,                                                    // xdeathstate
        SfxEnum::None,                                                       // deathsound
        0.0,                                                                 // speed
        20.0,                                                                // radius
        16.0,                                                                // height
        100,                                                                 // mass
        0,                                                                   // damage
        SfxEnum::None,                                                       // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::NotDeathmatch as u32, // flags
        StateNum::S_NULL,                                                    // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC8
        38,                                                                  // doomednum
        StateNum::S_RSKULL,                                                  // spawnstate
        1000,                                                                // spawnhealth
        StateNum::S_NULL,                                                    // seestate
        SfxEnum::None,                                                       // seesound
        8,                                                                   // reactiontime
        SfxEnum::None,                                                       // attacksound
        StateNum::S_NULL,                                                    // painstate
        0,                                                                   // painchance
        SfxEnum::None,                                                       // painsound
        StateNum::S_NULL,                                                    // meleestate
        StateNum::S_NULL,                                                    // missilestate
        StateNum::S_NULL,                                                    // deathstate
        StateNum::S_NULL,                                                    // xdeathstate
        SfxEnum::None,                                                       // deathsound
        0.0,                                                                 // speed
        20.0,                                                                // radius
        16.0,                                                                // height
        100,                                                                 // mass
        0,                                                                   // damage
        SfxEnum::None,                                                       // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::NotDeathmatch as u32, // flags
        StateNum::S_NULL,                                                    // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC9
        40,                                                                  // doomednum
        StateNum::S_BSKULL,                                                  // spawnstate
        1000,                                                                // spawnhealth
        StateNum::S_NULL,                                                    // seestate
        SfxEnum::None,                                                       // seesound
        8,                                                                   // reactiontime
        SfxEnum::None,                                                       // attacksound
        StateNum::S_NULL,                                                    // painstate
        0,                                                                   // painchance
        SfxEnum::None,                                                       // painsound
        StateNum::S_NULL,                                                    // meleestate
        StateNum::S_NULL,                                                    // missilestate
        StateNum::S_NULL,                                                    // deathstate
        StateNum::S_NULL,                                                    // xdeathstate
        SfxEnum::None,                                                       // deathsound
        0.0,                                                                 // speed
        20.0,                                                                // radius
        16.0,                                                                // height
        100,                                                                 // mass
        0,                                                                   // damage
        SfxEnum::None,                                                       // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::NotDeathmatch as u32, // flags
        StateNum::S_NULL,                                                    // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC10
        2011,                          // doomednum
        StateNum::S_STIM,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC11
        2012,                          // doomednum
        StateNum::S_MEDI,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC12
        2013,                                                            // doomednum
        StateNum::S_SOUL,                                                // spawnstate
        1000,                                                            // spawnhealth
        StateNum::S_NULL,                                                // seestate
        SfxEnum::None,                                                   // seesound
        8,                                                               // reactiontime
        SfxEnum::None,                                                   // attacksound
        StateNum::S_NULL,                                                // painstate
        0,                                                               // painchance
        SfxEnum::None,                                                   // painsound
        StateNum::S_NULL,                                                // meleestate
        StateNum::S_NULL,                                                // missilestate
        StateNum::S_NULL,                                                // deathstate
        StateNum::S_NULL,                                                // xdeathstate
        SfxEnum::None,                                                   // deathsound
        0.0,                                                             // speed
        20.0,                                                            // radius
        16.0,                                                            // height
        100,                                                             // mass
        0,                                                               // damage
        SfxEnum::None,                                                   // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::CountItem as u32, // flags
        StateNum::S_NULL,                                                // raisestate
    ),
    MapObjectInfo::new(
        // MT_INV
        2022,                                                            // doomednum
        StateNum::S_PINV,                                                // spawnstate
        1000,                                                            // spawnhealth
        StateNum::S_NULL,                                                // seestate
        SfxEnum::None,                                                   // seesound
        8,                                                               // reactiontime
        SfxEnum::None,                                                   // attacksound
        StateNum::S_NULL,                                                // painstate
        0,                                                               // painchance
        SfxEnum::None,                                                   // painsound
        StateNum::S_NULL,                                                // meleestate
        StateNum::S_NULL,                                                // missilestate
        StateNum::S_NULL,                                                // deathstate
        StateNum::S_NULL,                                                // xdeathstate
        SfxEnum::None,                                                   // deathsound
        0.0,                                                             // speed
        20.0,                                                            // radius
        16.0,                                                            // height
        100,                                                             // mass
        0,                                                               // damage
        SfxEnum::None,                                                   // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::CountItem as u32, // flags
        StateNum::S_NULL,                                                // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC13
        2023,                                                            // doomednum
        StateNum::S_PSTR,                                                // spawnstate
        1000,                                                            // spawnhealth
        StateNum::S_NULL,                                                // seestate
        SfxEnum::None,                                                   // seesound
        8,                                                               // reactiontime
        SfxEnum::None,                                                   // attacksound
        StateNum::S_NULL,                                                // painstate
        0,                                                               // painchance
        SfxEnum::None,                                                   // painsound
        StateNum::S_NULL,                                                // meleestate
        StateNum::S_NULL,                                                // missilestate
        StateNum::S_NULL,                                                // deathstate
        StateNum::S_NULL,                                                // xdeathstate
        SfxEnum::None,                                                   // deathsound
        0.0,                                                             // speed
        20.0,                                                            // radius
        16.0,                                                            // height
        100,                                                             // mass
        0,                                                               // damage
        SfxEnum::None,                                                   // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::CountItem as u32, // flags
        StateNum::S_NULL,                                                // raisestate
    ),
    MapObjectInfo::new(
        // MT_INS
        2024,                                                            // doomednum
        StateNum::S_PINS,                                                // spawnstate
        1000,                                                            // spawnhealth
        StateNum::S_NULL,                                                // seestate
        SfxEnum::None,                                                   // seesound
        8,                                                               // reactiontime
        SfxEnum::None,                                                   // attacksound
        StateNum::S_NULL,                                                // painstate
        0,                                                               // painchance
        SfxEnum::None,                                                   // painsound
        StateNum::S_NULL,                                                // meleestate
        StateNum::S_NULL,                                                // missilestate
        StateNum::S_NULL,                                                // deathstate
        StateNum::S_NULL,                                                // xdeathstate
        SfxEnum::None,                                                   // deathsound
        0.0,                                                             // speed
        20.0,                                                            // radius
        16.0,                                                            // height
        100,                                                             // mass
        0,                                                               // damage
        SfxEnum::None,                                                   // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::CountItem as u32, // flags
        StateNum::S_NULL,                                                // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC14
        2025,                          // doomednum
        StateNum::S_SUIT,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC15
        2026,                                                            // doomednum
        StateNum::S_PMAP,                                                // spawnstate
        1000,                                                            // spawnhealth
        StateNum::S_NULL,                                                // seestate
        SfxEnum::None,                                                   // seesound
        8,                                                               // reactiontime
        SfxEnum::None,                                                   // attacksound
        StateNum::S_NULL,                                                // painstate
        0,                                                               // painchance
        SfxEnum::None,                                                   // painsound
        StateNum::S_NULL,                                                // meleestate
        StateNum::S_NULL,                                                // missilestate
        StateNum::S_NULL,                                                // deathstate
        StateNum::S_NULL,                                                // xdeathstate
        SfxEnum::None,                                                   // deathsound
        0.0,                                                             // speed
        20.0,                                                            // radius
        16.0,                                                            // height
        100,                                                             // mass
        0,                                                               // damage
        SfxEnum::None,                                                   // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::CountItem as u32, // flags
        StateNum::S_NULL,                                                // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC16
        2045,                                                            // doomednum
        StateNum::S_PVIS,                                                // spawnstate
        1000,                                                            // spawnhealth
        StateNum::S_NULL,                                                // seestate
        SfxEnum::None,                                                   // seesound
        8,                                                               // reactiontime
        SfxEnum::None,                                                   // attacksound
        StateNum::S_NULL,                                                // painstate
        0,                                                               // painchance
        SfxEnum::None,                                                   // painsound
        StateNum::S_NULL,                                                // meleestate
        StateNum::S_NULL,                                                // missilestate
        StateNum::S_NULL,                                                // deathstate
        StateNum::S_NULL,                                                // xdeathstate
        SfxEnum::None,                                                   // deathsound
        0.0,                                                             // speed
        20.0,                                                            // radius
        16.0,                                                            // height
        100,                                                             // mass
        0,                                                               // damage
        SfxEnum::None,                                                   // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::CountItem as u32, // flags
        StateNum::S_NULL,                                                // raisestate
    ),
    MapObjectInfo::new(
        // MT_MEGA
        83,                                                              // doomednum
        StateNum::S_MEGA,                                                // spawnstate
        1000,                                                            // spawnhealth
        StateNum::S_NULL,                                                // seestate
        SfxEnum::None,                                                   // seesound
        8,                                                               // reactiontime
        SfxEnum::None,                                                   // attacksound
        StateNum::S_NULL,                                                // painstate
        0,                                                               // painchance
        SfxEnum::None,                                                   // painsound
        StateNum::S_NULL,                                                // meleestate
        StateNum::S_NULL,                                                // missilestate
        StateNum::S_NULL,                                                // deathstate
        StateNum::S_NULL,                                                // xdeathstate
        SfxEnum::None,                                                   // deathsound
        0.0,                                                             // speed
        20.0,                                                            // radius
        16.0,                                                            // height
        100,                                                             // mass
        0,                                                               // damage
        SfxEnum::None,                                                   // activesound
        MapObjectFlag::Special as u32 | MapObjectFlag::CountItem as u32, // flags
        StateNum::S_NULL,                                                // raisestate
    ),
    MapObjectInfo::new(
        // MT_CLIP
        2007,                          // doomednum
        StateNum::S_CLIP,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC17
        2048,                          // doomednum
        StateNum::S_AMMO,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC18
        2010,                          // doomednum
        StateNum::S_ROCK,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC19
        2046,                          // doomednum
        StateNum::S_BROK,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC20
        2047,                          // doomednum
        StateNum::S_CELL,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC21
        17,                            // doomednum
        StateNum::S_CELP,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC22
        2008,                          // doomednum
        StateNum::S_SHEL,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC23
        2049,                          // doomednum
        StateNum::S_SBOX,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC24
        8,                             // doomednum
        StateNum::S_BPAK,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC25
        2006,                          // doomednum
        StateNum::S_BFUG,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_CHAINGUN
        2002,                          // doomednum
        StateNum::S_MGUN,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC26
        2005,                          // doomednum
        StateNum::S_CSAW,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC27
        2003,                          // doomednum
        StateNum::S_LAUN,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC28
        2004,                          // doomednum
        StateNum::S_PLAS,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_SHOTGUN
        2001,                          // doomednum
        StateNum::S_SHOT,              // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_SUPERSHOTGUN
        82,                            // doomednum
        StateNum::S_SHOT2,             // spawnstate
        1000,                          // spawnhealth
        StateNum::S_NULL,              // seestate
        SfxEnum::None,                 // seesound
        8,                             // reactiontime
        SfxEnum::None,                 // attacksound
        StateNum::S_NULL,              // painstate
        0,                             // painchance
        SfxEnum::None,                 // painsound
        StateNum::S_NULL,              // meleestate
        StateNum::S_NULL,              // missilestate
        StateNum::S_NULL,              // deathstate
        StateNum::S_NULL,              // xdeathstate
        SfxEnum::None,                 // deathsound
        0.0,                           // speed
        20.0,                          // radius
        16.0,                          // height
        100,                           // mass
        0,                             // damage
        SfxEnum::None,                 // activesound
        MapObjectFlag::Special as u32, // flags
        StateNum::S_NULL,              // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC29
        85,                          // doomednum
        StateNum::S_TECHLAMP,        // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC30
        86,                          // doomednum
        StateNum::S_TECH2LAMP,       // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC31
        2028,                        // doomednum
        StateNum::S_COLU,            // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC32
        30,                          // doomednum
        StateNum::S_TALLGRNCOL,      // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC33
        31,                          // doomednum
        StateNum::S_SHRTGRNCOL,      // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC34
        32,                          // doomednum
        StateNum::S_TALLREDCOL,      // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC35
        33,                          // doomednum
        StateNum::S_SHRTREDCOL,      // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC36
        37,                          // doomednum
        StateNum::S_SKULLCOL,        // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC37
        36,                          // doomednum
        StateNum::S_HEARTCOL,        // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC38
        41,                          // doomednum
        StateNum::S_EVILEYE,         // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC39
        42,                          // doomednum
        StateNum::S_FLOATSKULL,      // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC40
        43,                          // doomednum
        StateNum::S_TORCHTREE,       // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC41
        44,                          // doomednum
        StateNum::S_BLUETORCH,       // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC42
        45,                          // doomednum
        StateNum::S_GREENTORCH,      // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC43
        46,                          // doomednum
        StateNum::S_REDTORCH,        // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC44
        55,                          // doomednum
        StateNum::S_BTORCHSHRT,      // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC45
        56,                          // doomednum
        StateNum::S_GTORCHSHRT,      // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC46
        57,                          // doomednum
        StateNum::S_RTORCHSHRT,      // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC47
        47,                          // doomednum
        StateNum::S_STALAGTITE,      // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC48
        48,                          // doomednum
        StateNum::S_TECHPILLAR,      // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC49
        34,                     // doomednum
        StateNum::S_CANDLESTIK, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::None,          // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::None,          // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::None,          // deathsound
        0.0,                    // speed
        20.0,                   // radius
        16.0,                   // height
        100,                    // mass
        0,                      // damage
        SfxEnum::None,          // activesound
        0,                      // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC50
        35,                          // doomednum
        StateNum::S_CANDELABRA,      // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC51
        49,                       // doomednum
        StateNum::S_BLOODYTWITCH, // spawnstate
        1000,                     // spawnhealth
        StateNum::S_NULL,         // seestate
        SfxEnum::None,            // seesound
        8,                        // reactiontime
        SfxEnum::None,            // attacksound
        StateNum::S_NULL,         // painstate
        0,                        // painchance
        SfxEnum::None,            // painsound
        StateNum::S_NULL,         // meleestate
        StateNum::S_NULL,         // missilestate
        StateNum::S_NULL,         // deathstate
        StateNum::S_NULL,         // xdeathstate
        SfxEnum::None,            // deathsound
        0.0,                      // speed
        16.0,                     // radius
        68.0,                     // height
        100,                      // mass
        0,                        // damage
        SfxEnum::None,            // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::SpawnCeiling as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,         // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC52
        50,                // doomednum
        StateNum::S_MEAT2, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::None,     // seesound
        8,                 // reactiontime
        SfxEnum::None,     // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::None,     // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::None,     // deathsound
        0.0,               // speed
        16.0,              // radius
        84.0,              // height
        100,               // mass
        0,                 // damage
        SfxEnum::None,     // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::SpawnCeiling as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC53
        51,                // doomednum
        StateNum::S_MEAT3, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::None,     // seesound
        8,                 // reactiontime
        SfxEnum::None,     // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::None,     // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::None,     // deathsound
        0.0,               // speed
        16.0,              // radius
        84.0,              // height
        100,               // mass
        0,                 // damage
        SfxEnum::None,     // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::SpawnCeiling as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC54
        52,                // doomednum
        StateNum::S_MEAT4, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::None,     // seesound
        8,                 // reactiontime
        SfxEnum::None,     // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::None,     // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::None,     // deathsound
        0.0,               // speed
        16.0,              // radius
        68.0,              // height
        100,               // mass
        0,                 // damage
        SfxEnum::None,     // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::SpawnCeiling as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC55
        53,                // doomednum
        StateNum::S_MEAT5, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::None,     // seesound
        8,                 // reactiontime
        SfxEnum::None,     // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::None,     // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::None,     // deathsound
        0.0,               // speed
        16.0,              // radius
        52.0,              // height
        100,               // mass
        0,                 // damage
        SfxEnum::None,     // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::SpawnCeiling as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC56
        59,                                                                   // doomednum
        StateNum::S_MEAT2,                                                    // spawnstate
        1000,                                                                 // spawnhealth
        StateNum::S_NULL,                                                     // seestate
        SfxEnum::None,                                                        // seesound
        8,                                                                    // reactiontime
        SfxEnum::None,                                                        // attacksound
        StateNum::S_NULL,                                                     // painstate
        0,                                                                    // painchance
        SfxEnum::None,                                                        // painsound
        StateNum::S_NULL,                                                     // meleestate
        StateNum::S_NULL,                                                     // missilestate
        StateNum::S_NULL,                                                     // deathstate
        StateNum::S_NULL,                                                     // xdeathstate
        SfxEnum::None,                                                        // deathsound
        0.0,                                                                  // speed
        20.0,                                                                 // radius
        84.0,                                                                 // height
        100,                                                                  // mass
        0,                                                                    // damage
        SfxEnum::None,                                                        // activesound
        MapObjectFlag::SpawnCeiling as u32 | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,                                                     // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC57
        60,                                                                   // doomednum
        StateNum::S_MEAT4,                                                    // spawnstate
        1000,                                                                 // spawnhealth
        StateNum::S_NULL,                                                     // seestate
        SfxEnum::None,                                                        // seesound
        8,                                                                    // reactiontime
        SfxEnum::None,                                                        // attacksound
        StateNum::S_NULL,                                                     // painstate
        0,                                                                    // painchance
        SfxEnum::None,                                                        // painsound
        StateNum::S_NULL,                                                     // meleestate
        StateNum::S_NULL,                                                     // missilestate
        StateNum::S_NULL,                                                     // deathstate
        StateNum::S_NULL,                                                     // xdeathstate
        SfxEnum::None,                                                        // deathsound
        0.0,                                                                  // speed
        20.0,                                                                 // radius
        68.0,                                                                 // height
        100,                                                                  // mass
        0,                                                                    // damage
        SfxEnum::None,                                                        // activesound
        MapObjectFlag::SpawnCeiling as u32 | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,                                                     // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC58
        61,                                                                   // doomednum
        StateNum::S_MEAT3,                                                    // spawnstate
        1000,                                                                 // spawnhealth
        StateNum::S_NULL,                                                     // seestate
        SfxEnum::None,                                                        // seesound
        8,                                                                    // reactiontime
        SfxEnum::None,                                                        // attacksound
        StateNum::S_NULL,                                                     // painstate
        0,                                                                    // painchance
        SfxEnum::None,                                                        // painsound
        StateNum::S_NULL,                                                     // meleestate
        StateNum::S_NULL,                                                     // missilestate
        StateNum::S_NULL,                                                     // deathstate
        StateNum::S_NULL,                                                     // xdeathstate
        SfxEnum::None,                                                        // deathsound
        0.0,                                                                  // speed
        20.0,                                                                 // radius
        52.0,                                                                 // height
        100,                                                                  // mass
        0,                                                                    // damage
        SfxEnum::None,                                                        // activesound
        MapObjectFlag::SpawnCeiling as u32 | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,                                                     // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC59
        62,                                                                   // doomednum
        StateNum::S_MEAT5,                                                    // spawnstate
        1000,                                                                 // spawnhealth
        StateNum::S_NULL,                                                     // seestate
        SfxEnum::None,                                                        // seesound
        8,                                                                    // reactiontime
        SfxEnum::None,                                                        // attacksound
        StateNum::S_NULL,                                                     // painstate
        0,                                                                    // painchance
        SfxEnum::None,                                                        // painsound
        StateNum::S_NULL,                                                     // meleestate
        StateNum::S_NULL,                                                     // missilestate
        StateNum::S_NULL,                                                     // deathstate
        StateNum::S_NULL,                                                     // xdeathstate
        SfxEnum::None,                                                        // deathsound
        0.0,                                                                  // speed
        20.0,                                                                 // radius
        52.0,                                                                 // height
        100,                                                                  // mass
        0,                                                                    // damage
        SfxEnum::None,                                                        // activesound
        MapObjectFlag::SpawnCeiling as u32 | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,                                                     // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC60
        63,                                                                   // doomednum
        StateNum::S_BLOODYTWITCH,                                             // spawnstate
        1000,                                                                 // spawnhealth
        StateNum::S_NULL,                                                     // seestate
        SfxEnum::None,                                                        // seesound
        8,                                                                    // reactiontime
        SfxEnum::None,                                                        // attacksound
        StateNum::S_NULL,                                                     // painstate
        0,                                                                    // painchance
        SfxEnum::None,                                                        // painsound
        StateNum::S_NULL,                                                     // meleestate
        StateNum::S_NULL,                                                     // missilestate
        StateNum::S_NULL,                                                     // deathstate
        StateNum::S_NULL,                                                     // xdeathstate
        SfxEnum::None,                                                        // deathsound
        0.0,                                                                  // speed
        20.0,                                                                 // radius
        68.0,                                                                 // height
        100,                                                                  // mass
        0,                                                                    // damage
        SfxEnum::None,                                                        // activesound
        MapObjectFlag::SpawnCeiling as u32 | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,                                                     // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC61
        22,                    // doomednum
        StateNum::S_HEAD_DIE6, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::None,         // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::None,         // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_NULL,      // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::None,         // deathsound
        0.0,                   // speed
        20.0,                  // radius
        16.0,                  // height
        100,                   // mass
        0,                     // damage
        SfxEnum::None,         // activesound
        0,                     // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC62
        15,                    // doomednum
        StateNum::S_PLAY_DIE7, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::None,         // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::None,         // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_NULL,      // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::None,         // deathsound
        0.0,                   // speed
        20.0,                  // radius
        16.0,                  // height
        100,                   // mass
        0,                     // damage
        SfxEnum::None,         // activesound
        0,                     // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC63
        18,                    // doomednum
        StateNum::S_POSS_DIE5, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::None,         // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::None,         // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_NULL,      // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::None,         // deathsound
        0.0,                   // speed
        20.0,                  // radius
        16.0,                  // height
        100,                   // mass
        0,                     // damage
        SfxEnum::None,         // activesound
        0,                     // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC64
        21,                    // doomednum
        StateNum::S_SARG_DIE6, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::None,         // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::None,         // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_NULL,      // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::None,         // deathsound
        0.0,                   // speed
        20.0,                  // radius
        16.0,                  // height
        100,                   // mass
        0,                     // damage
        SfxEnum::None,         // activesound
        0,                     // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC65
        23,                     // doomednum
        StateNum::S_SKULL_DIE6, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::None,          // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::None,          // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::None,          // deathsound
        0.0,                    // speed
        20.0,                   // radius
        16.0,                   // height
        100,                    // mass
        0,                      // damage
        SfxEnum::None,          // activesound
        0,                      // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC66
        20,                    // doomednum
        StateNum::S_TROO_DIE5, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::None,         // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::None,         // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_NULL,      // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::None,         // deathsound
        0.0,                   // speed
        20.0,                  // radius
        16.0,                  // height
        100,                   // mass
        0,                     // damage
        SfxEnum::None,         // activesound
        0,                     // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC67
        19,                    // doomednum
        StateNum::S_POSS_DIE5, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::None,         // seesound
        8,                     // reactiontime
        SfxEnum::None,         // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::None,         // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_NULL,      // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::None,         // deathsound
        0.0,                   // speed
        20.0,                  // radius
        16.0,                  // height
        100,                   // mass
        0,                     // damage
        SfxEnum::None,         // activesound
        0,                     // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC68
        10,                     // doomednum
        StateNum::S_PLAY_XDIE9, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::None,          // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::None,          // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::None,          // deathsound
        0.0,                    // speed
        20.0,                   // radius
        16.0,                   // height
        100,                    // mass
        0,                      // damage
        SfxEnum::None,          // activesound
        0,                      // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC69
        12,                     // doomednum
        StateNum::S_PLAY_XDIE9, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::None,          // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::None,          // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::None,          // deathsound
        0.0,                    // speed
        20.0,                   // radius
        16.0,                   // height
        100,                    // mass
        0,                      // damage
        SfxEnum::None,          // activesound
        0,                      // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC70
        28,                          // doomednum
        StateNum::S_HEADSONSTICK,    // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC71
        24,               // doomednum
        StateNum::S_GIBS, // spawnstate
        1000,             // spawnhealth
        StateNum::S_NULL, // seestate
        SfxEnum::None,    // seesound
        8,                // reactiontime
        SfxEnum::None,    // attacksound
        StateNum::S_NULL, // painstate
        0,                // painchance
        SfxEnum::None,    // painsound
        StateNum::S_NULL, // meleestate
        StateNum::S_NULL, // missilestate
        StateNum::S_NULL, // deathstate
        StateNum::S_NULL, // xdeathstate
        SfxEnum::None,    // deathsound
        0.0,              // speed
        20.0,             // radius
        16.0,             // height
        100,              // mass
        0,                // damage
        SfxEnum::None,    // activesound
        0,                // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC72
        27,                          // doomednum
        StateNum::S_HEADONASTICK,    // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC73
        29,                          // doomednum
        StateNum::S_HEADCANDLES,     // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC74
        25,                          // doomednum
        StateNum::S_DEADSTICK,       // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC75
        26,                          // doomednum
        StateNum::S_LIVESTICK,       // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC76
        54,                          // doomednum
        StateNum::S_BIGTREE,         // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        32.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC77
        70,                          // doomednum
        StateNum::S_BBAR1,           // spawnstate
        1000,                        // spawnhealth
        StateNum::S_NULL,            // seestate
        SfxEnum::None,               // seesound
        8,                           // reactiontime
        SfxEnum::None,               // attacksound
        StateNum::S_NULL,            // painstate
        0,                           // painchance
        SfxEnum::None,               // painsound
        StateNum::S_NULL,            // meleestate
        StateNum::S_NULL,            // missilestate
        StateNum::S_NULL,            // deathstate
        StateNum::S_NULL,            // xdeathstate
        SfxEnum::None,               // deathsound
        0.0,                         // speed
        16.0,                        // radius
        16.0,                        // height
        100,                         // mass
        0,                           // damage
        SfxEnum::None,               // activesound
        MapObjectFlag::Solid as u32, // flags
        StateNum::S_NULL,            // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC78
        73,                     // doomednum
        StateNum::S_HANGNOGUTS, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::None,          // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::None,          // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::None,          // deathsound
        0.0,                    // speed
        16.0,                   // radius
        88.0,                   // height
        100,                    // mass
        0,                      // damage
        SfxEnum::None,          // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::SpawnCeiling as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC79
        74,                       // doomednum
        StateNum::S_HANGBNOBRAIN, // spawnstate
        1000,                     // spawnhealth
        StateNum::S_NULL,         // seestate
        SfxEnum::None,            // seesound
        8,                        // reactiontime
        SfxEnum::None,            // attacksound
        StateNum::S_NULL,         // painstate
        0,                        // painchance
        SfxEnum::None,            // painsound
        StateNum::S_NULL,         // meleestate
        StateNum::S_NULL,         // missilestate
        StateNum::S_NULL,         // deathstate
        StateNum::S_NULL,         // xdeathstate
        SfxEnum::None,            // deathsound
        0.0,                      // speed
        16.0,                     // radius
        88.0,                     // height
        100,                      // mass
        0,                        // damage
        SfxEnum::None,            // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::SpawnCeiling as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,         // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC80
        75,                      // doomednum
        StateNum::S_HANGTLOOKDN, // spawnstate
        1000,                    // spawnhealth
        StateNum::S_NULL,        // seestate
        SfxEnum::None,           // seesound
        8,                       // reactiontime
        SfxEnum::None,           // attacksound
        StateNum::S_NULL,        // painstate
        0,                       // painchance
        SfxEnum::None,           // painsound
        StateNum::S_NULL,        // meleestate
        StateNum::S_NULL,        // missilestate
        StateNum::S_NULL,        // deathstate
        StateNum::S_NULL,        // xdeathstate
        SfxEnum::None,           // deathsound
        0.0,                     // speed
        16.0,                    // radius
        64.0,                    // height
        100,                     // mass
        0,                       // damage
        SfxEnum::None,           // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::SpawnCeiling as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,        // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC81
        76,                     // doomednum
        StateNum::S_HANGTSKULL, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::None,          // seesound
        8,                      // reactiontime
        SfxEnum::None,          // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::None,          // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::None,          // deathsound
        0.0,                    // speed
        16.0,                   // radius
        64.0,                   // height
        100,                    // mass
        0,                      // damage
        SfxEnum::None,          // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::SpawnCeiling as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC82
        77,                      // doomednum
        StateNum::S_HANGTLOOKUP, // spawnstate
        1000,                    // spawnhealth
        StateNum::S_NULL,        // seestate
        SfxEnum::None,           // seesound
        8,                       // reactiontime
        SfxEnum::None,           // attacksound
        StateNum::S_NULL,        // painstate
        0,                       // painchance
        SfxEnum::None,           // painsound
        StateNum::S_NULL,        // meleestate
        StateNum::S_NULL,        // missilestate
        StateNum::S_NULL,        // deathstate
        StateNum::S_NULL,        // xdeathstate
        SfxEnum::None,           // deathsound
        0.0,                     // speed
        16.0,                    // radius
        64.0,                    // height
        100,                     // mass
        0,                       // damage
        SfxEnum::None,           // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::SpawnCeiling as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,        // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC83
        78,                       // doomednum
        StateNum::S_HANGTNOBRAIN, // spawnstate
        1000,                     // spawnhealth
        StateNum::S_NULL,         // seestate
        SfxEnum::None,            // seesound
        8,                        // reactiontime
        SfxEnum::None,            // attacksound
        StateNum::S_NULL,         // painstate
        0,                        // painchance
        SfxEnum::None,            // painsound
        StateNum::S_NULL,         // meleestate
        StateNum::S_NULL,         // missilestate
        StateNum::S_NULL,         // deathstate
        StateNum::S_NULL,         // xdeathstate
        SfxEnum::None,            // deathsound
        0.0,                      // speed
        16.0,                     // radius
        64.0,                     // height
        100,                      // mass
        0,                        // damage
        SfxEnum::None,            // activesound
        MapObjectFlag::Solid as u32
            | MapObjectFlag::SpawnCeiling as u32
            | MapObjectFlag::NoGravity as u32, // flags
        StateNum::S_NULL,         // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC84
        79,                               // doomednum
        StateNum::S_COLONGIBS,            // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::None,                    // seesound
        8,                                // reactiontime
        SfxEnum::None,                    // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::None,                    // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::None,                    // deathsound
        0.0,                              // speed
        20.0,                             // radius
        16.0,                             // height
        100,                              // mass
        0,                                // damage
        SfxEnum::None,                    // activesound
        MapObjectFlag::NoBlockMap as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC85
        80,                               // doomednum
        StateNum::S_SMALLPOOL,            // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::None,                    // seesound
        8,                                // reactiontime
        SfxEnum::None,                    // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::None,                    // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::None,                    // deathsound
        0.0,                              // speed
        20.0,                             // radius
        16.0,                             // height
        100,                              // mass
        0,                                // damage
        SfxEnum::None,                    // activesound
        MapObjectFlag::NoBlockMap as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC86
        81,                               // doomednum
        StateNum::S_BRAINSTEM,            // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::None,                    // seesound
        8,                                // reactiontime
        SfxEnum::None,                    // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::None,                    // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::None,                    // deathsound
        0.0,                              // speed
        20.0,                             // radius
        16.0,                             // height
        100,                              // mass
        0,                                // damage
        SfxEnum::None,                    // activesound
        MapObjectFlag::NoBlockMap as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
];
