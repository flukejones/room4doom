use log::debug;
use wad::{lumps::WadThing, WadData};

use crate::d_thinker::ThinkerAlloc;
use crate::doom_def::GameAction;
use crate::level_data::map_data::MapData;
use crate::renderer::bsp::BspRenderer;
use crate::renderer::plane::VisPlaneCtrl;
use crate::renderer::RenderData;
use crate::{
    d_main::Skill, doom_def::GameMode, doom_def::MAXPLAYERS, doom_def::MAX_DEATHMATCH_STARTS,
    game::Game,
};

/// The level is considered a `World` or sorts. One that exists only
/// while the player is in it. Another benefit of this structure is
/// it makes it easier for all involved thinkers and functions to
/// work with the data, as much of it is interlinked.
///
/// In some ways this is the "P" module
pub struct Level {
    pub map_data: MapData,
    pub bsp_renderer: BspRenderer,
    pub r_data: RenderData,
    pub visplanes: VisPlaneCtrl,
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
}
impl Level {
    /// Set up a complete level including difficulty, spawns, players etc.
    /// After `new()` the `load()` function should be called.
    ///
    /// # Safety
    /// Because the `Level` uses ` ThinkerAlloc` internally the `Level` must not
    /// be moved by the owner after any thinkers are pushed to `ThinkerAlloc`.
    /// This applies to the map data also where `load()` should be called after
    /// the locations is set in concrete.
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
            r_data: RenderData::default(),
            visplanes: VisPlaneCtrl::default(),
            bsp_renderer: BspRenderer::default(),
            thinkers: unsafe { ThinkerAlloc::new(thinker_count + 500) },
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
        }
        // TODO: P_InitThinkers();
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

        // this block is P_RunThinkers()
        // TODO: maybe use direct linked list iter here so we can remove while iterating
        let lev = unsafe { &mut *(level as *mut Level) };
        let mut rm = Vec::with_capacity(level.thinkers.len());

        // TODO: can't modify linked list when iterating as the iter holds state that can't be updated
        for thinker in level.thinkers.iter_mut() {
            thinker.think(lev);
            if thinker.remove() {
                rm.push(thinker.index());
            }
        }
        for idx in rm {
            debug!("Removing: {idx}");
            level.thinkers.remove(idx);
        }

        level.level_time += 1;
    }

    // P_UpdateSpecials ();
    // P_RespawnSpecials ();
}
