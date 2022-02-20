use std::marker::PhantomPinned;

use log::debug;
use wad::{lumps::WadThing, WadData};

use crate::{
    d_main::Skill,
    doom_def::{GameAction, GameMode, MAXPLAYERS, MAX_DEATHMATCH_STARTS},
    game::Game,
    level_data::map_data::MapData,
    play::d_thinker::ThinkerAlloc,
};

/// The level is considered a `World` or sorts. One that exists only
/// while the player is in it. Another benefit of this structure is
/// it makes it easier for all involved thinkers and functions to
/// work with the data, as much of it is interlinked.
pub struct Level {
    pub map_data: MapData,
    pub thinkers: ThinkerAlloc,
    pub game_skill: Skill,
    pub respawn_monsters: bool,
    pub level_time: u32,
    /// Required for the mobj controller (Boss check)
    pub episode: u32,
    /// Required for the mobj controller (Boss check)
    pub game_map: u32,
    /// This needs to be synced with `Game`
    pub game_tic: u32,
    /// The `Things` for player start locations
    pub player_starts: [Option<WadThing>; MAXPLAYERS],
    /// The `Things` for deathmatch start locations
    pub deathmatch_starts: [Option<WadThing>; MAX_DEATHMATCH_STARTS],
    pub deathmatch_p: Vec<WadThing>,
    /// Was the level set for deathmatch game
    pub deathmatch: bool,
    /// for intermission
    pub totalkills: i32,
    /// for intermission
    pub totalitems: i32,
    /// for intermission
    pub totalsecret: i32,
    /// To change the game state via switches in the level
    pub game_action: Option<GameAction>,
    /// Record how the level was exited
    pub secret_exit: bool,
    /// Marker count for lines checked
    pub valid_count: usize,
    _pinned: PhantomPinned,
}
impl Level {
    /// Set up a complete level including difficulty, spawns, players etc.
    /// After `new()` the `load()` function should be called.
    ///
    /// # Safety
    /// Because the `Level` uses ` ThinkerAlloc` internally the `Level` must not
    /// be moved by the owner after any thinkers are pushed to `ThinkerAlloc`.
    /// This applies to the map data also where `load()` should be called after
    /// the locations is set in concrete. Other common tasks to do after `new()`
    /// are spawning specials, things.
    ///
    /// Doom method name is `P_SetupLevel`
    pub unsafe fn new(skill: Skill, episode: u32, map: u32, game_mode: GameMode) -> Self {
        let respawn_monsters = !matches!(skill, Skill::Nightmare);

        let map_name = if game_mode == GameMode::Commercial {
            if map < 10 {
                format!("MAP0{}", map)
            } else {
                format!("MAP{}", map)
            }
        } else {
            format!("E{}M{}", episode, map)
        };

        let map_data = MapData::new(map_name);

        let thinker_count = map_data.get_things().len();

        // G_DoReborn
        // G_CheckSpot

        Level {
            map_data,
            thinkers: ThinkerAlloc::new(thinker_count + 500),
            game_skill: skill,
            respawn_monsters,
            level_time: 0,
            episode,
            game_map: map,
            game_tic: 0,
            player_starts: [None; MAXPLAYERS],
            deathmatch_starts: [None; MAX_DEATHMATCH_STARTS],
            deathmatch_p: Vec::with_capacity(MAX_DEATHMATCH_STARTS),
            deathmatch: false,
            // TODO: copy end values to game obj
            totalkills: 0,
            totalitems: 0,
            totalsecret: 0,
            game_action: None,
            secret_exit: false,
            valid_count: 0,
            _pinned: PhantomPinned,
        }
    }

    pub fn load(&mut self, wad_data: &WadData) {
        self.map_data.load(wad_data);
    }

    // pub fn add_thinker<T: Think>(&self, thinker: Thinker) -> Option<NonNull<Thinker>> {
    //     // TODO: do cleaning pass if can't insert
    //     let thinkers = &self.thinkers as *const ThinkerAlloc as *mut ThinkerAlloc;
    //     // Absolutely fucking with lifetimes here
    //     unsafe { (*thinkers).push::<T>(thinker) }
    // }

    pub fn do_exit_level(&mut self) {
        debug!("Exited level");
        self.secret_exit = false;
        self.game_action = Some(GameAction::ga_completed);
    }

    pub fn do_secret_exit_level(&mut self) {
        debug!("Secret exited level");
        self.secret_exit = true;
        self.game_action = Some(GameAction::ga_completed);
    }
}

/// P_Ticker
pub fn p_ticker(game: &mut Game) {
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
            if game.player_in_game[i] && !player.think(level) {
                // TODO: what to do with dead player?
            }
        }

        unsafe {
            let lev = &mut *(level as *mut Level);
            level.thinkers.run_thinkers(lev);

            // P_UpdateSpecials ();
            // P_RespawnSpecials ();
        }

        level.level_time += 1;
    }
}
