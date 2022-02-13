use std::ptr::NonNull;

use log::debug;
use wad::{lumps::WadThing, WadData};

use crate::d_thinker::ThinkerAlloc;
use crate::doom_def::GameAction;
use crate::level_data::map_data::MapData;
use crate::renderer::bsp::BspRenderer;
use crate::renderer::plane::VisPlaneCtrl;
use crate::renderer::RenderData;
use crate::{
    d_main::Skill,
    d_thinker::{Think, Thinker},
    doom_def::GameMode,
    doom_def::MAXPLAYERS,
    doom_def::MAX_DEATHMATCH_STARTS,
    game::Game,
    p_map::SubSectorMinMax,
    p_map_object::MapObject,
    player::Player,
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
    pub mobj_ctrl: SubSectorMinMax,
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
}
impl Level {
    /// P_SetupLevel
    pub fn setup_level(
        wad_data: &WadData,
        skill: Skill,
        episode: u32,
        map: u32,
        game_mode: GameMode,
        players: &mut [Player],
        active_players: &[bool; MAXPLAYERS],
    ) -> Self {
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

        let mut map_data = MapData::new(map_name);
        map_data.load(wad_data);

        let thinker_count = map_data.get_things().len();

        let mut level = Level {
            map_data,
            r_data: RenderData::default(),
            visplanes: VisPlaneCtrl::default(),
            bsp_renderer: BspRenderer::default(),
            mobj_ctrl: SubSectorMinMax::default(),
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
        };

        let thing_list = (*level.map_data.get_things()).to_owned();

        for thing in &thing_list {
            MapObject::p_spawn_map_thing(thing, &mut level, players, active_players);
        }

        debug!("Level: thinkers = {}", &level.thinkers.len());
        debug!("Level: skill = {:?}", &level.game_skill);
        debug!("Level: episode = {}", &level.episode);
        debug!("Level: map = {}", &level.game_map);
        debug!("Level: player_starts = {:?}", &level.player_starts);

        // G_DoReborn
        // G_CheckSpot

        level
        // TODO: P_InitThinkers();
    }

    pub fn add_thinker<T: Think>(&self, thinker: Thinker) -> Option<NonNull<Thinker>> {
        // TODO: do cleaning pass if can't insert
        let thinkers = &self.thinkers as *const ThinkerAlloc as *mut ThinkerAlloc;
        // Absolutely fucking with lifetimes here
        unsafe { (*thinkers).push::<T>(thinker) }
    }

    pub fn do_exit_level(&mut self) {
        debug!("Exited level");
        self.secret_exit = false;
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
        let l = unsafe { &mut *(level as *mut Level) };
        let mut rm = Vec::with_capacity(level.thinkers.len());
        for thinker in level.thinkers.iter_mut() {
            if thinker.has_action() {
                thinker.think(l);
            } else {
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
