use wad::Wad;

use crate::{
    d_main::Skill,
    d_thinker::Think,
    d_thinker::{ActionFunc, Thinker},
    doom_def::GameMode,
    game::Game,
    map_data::MapData,
    p_map::MobjCtrl,
    p_map_object::MapObject,
    player::Player,
    r_bsp::BspCtrl,
};

/// The level is considered a `World` or sorts. One that exists only
/// while the player is in it. Another benefit of this structure is
/// it makes it easier for all involved thinkers and functions to
/// work with the data, as much of it is interlinked.
///
/// In some ways this is the "P" module
pub struct Level {
    pub map_data:         MapData,
    pub bsp_ctrl:         BspCtrl,
    pub mobj_ctrl:        MobjCtrl,
    pub thinkers:         Vec<Option<Thinker<MapObject>>>,
    pub game_skill:       Skill,
    pub respawn_monsters: bool,
    pub level_time:       i32,
    /// Required for the mobj controller (Boss check)
    pub episode:          u32,
    /// Required for the mobj controller (Boss check)
    pub game_map:         u32,
    /// This needs to be synced with `Game`
    pub game_tic:         u32,
}
impl Level {
    /// P_SetupLevel
    pub fn setup_level(
        wad_data: &Wad,
        skill: Skill,
        mut episode: u32,
        mut map: u32,
        game_mode: GameMode,
        players: &mut [Player],
    ) -> Self {
        let respawn_monsters = match skill {
            Skill::Nightmare => false,
            _ => true,
        };
        dbg!(game_mode);

        if game_mode == GameMode::Retail {
            if episode > 4 {
                episode = 4;
            }
        } else if game_mode == GameMode::Shareware {
            if episode > 1 {
                episode = 1; // only start episode 1 on shareware
            }
        } else {
            if episode > 3 {
                episode = 3;
            }
        }

        if map > 9 && game_mode != GameMode::Commercial {
            map = 9;
        }

        let map_name = if game_mode == GameMode::Commercial {
            if map < 10 {
                format!("MAP0{}", map)
            } else {
                format!("MAP{}", map)
            }
        } else {
            format!("E{}M{}", episode, map)
        };

        let mut map_data = MapData::new(map_name);
        map_data.load(wad_data);

        // TODO: actually find the players
        let player_thing = map_data.get_things()[0].clone();

        let level = Level {
            map_data,
            bsp_ctrl: BspCtrl::default(),
            mobj_ctrl: MobjCtrl::default(),
            thinkers: Vec::with_capacity(200),
            game_skill: skill,
            respawn_monsters,
            level_time: 0,
            episode,
            game_map: map,
            game_tic: 0,
        };

        MapObject::p_spawn_player(&player_thing, &level.map_data, players);

        level
        // TODO: P_InitThinkers();
    }
}

/// P_Ticker
pub fn ticker(game: &mut Game) {
    if game.paused {
        return;
    }
    // TODO: pause if in menu and at least one tic has been run
    // if ( !netgame
    //     && menuactive
    //     && !demoplayback
    // if game.players[game.consoleplayer].viewz as i32 != 1 {
    //     return;
    // }

    // Only run thinkers if a level is loaded
    if let Some(ref mut level) = game.level {
        for (i, player) in game.players.iter_mut().enumerate() {
            if game.player_in_game[i] {
                if player.think(level) {
                    if let Some(ref mut mobj) = player.mo {
                        mobj.unlink();
                        mobj.function = ActionFunc::None;
                    }
                }
            }
        }

        // P_RunThinkers ();, this may need to remove thinkers..
        // P_UpdateSpecials ();
        // P_RespawnSpecials ();

        // TODO: trial removal of mobs
        for i in 0..level.thinkers.len() {
            // Do not remove if already None, or if has Action
            let status =
                level.thinkers[i].as_ref().map_or(
                    false,
                    |thinker| match thinker.function {
                        ActionFunc::None => true,
                        _ => false,
                    },
                );
            if status {
                // An item must always be replaced in place to prevent realloc of vec
                let mut thinker = level.thinkers[i].take().unwrap();
                thinker.unlink();
            }
        }
    }
}
