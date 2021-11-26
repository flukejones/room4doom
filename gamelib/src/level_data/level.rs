use wad::{lumps::WadThing, WadData};

use crate::level_data::map_data::MapData;
use crate::renderer::bsp::BspRenderer;
use crate::renderer::plane::VisPlaneCtrl;
use crate::renderer::RenderData;
use crate::{
    d_main::Skill,
    d_thinker::Think,
    d_thinker::{ActionFunc, Thinker},
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
    pub thinkers: Vec<Option<Thinker<MapObject>>>,
    max_thinker_capacity: usize,
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
}
impl Level {
    /// P_SetupLevel
    pub fn setup_level(
        wad_data: &WadData,
        skill: Skill,
        mut episode: u32,
        mut map: u32,
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
            thinkers: Vec::with_capacity(thinker_count + 50),
            max_thinker_capacity: thinker_count + 50,
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
        };

        let thing_list = (*level.map_data.get_things()).to_owned();

        for thing in &thing_list {
            MapObject::p_spawn_map_thing(thing, &mut level, players, active_players);
        }
        dbg!(&level.thinkers.len());

        // G_DoReborn
        // G_CheckSpot

        level
        // TODO: P_InitThinkers();
    }

    pub fn add_thinker(&mut self, thinker: Thinker<MapObject>) -> bool {
        let mut index = 0;
        for i in 0..self.thinkers.len() {
            if self.thinkers[i].is_none() {
                break;
            }
            index += 1;
        }
        if index < self.max_thinker_capacity {
            if index < self.thinkers.len() {
                self.thinkers[index] = Some(thinker);
            } else {
                self.thinkers.push(Some(thinker));
            }
            return true;
        }

        false
    }

    /// Clean out the inactive thinkers. This iterates the full allocation to find
    /// thinkers with `None` action. The list is between 50-300 usually, depending
    /// on the level.
    pub fn clean_thinker_list(&mut self) {
        for i in 0..self.thinkers.len() {
            // Do not remove if already None, or if has Action
            let status = self.thinkers[i].as_ref().map_or(false, |thinker| {
                matches!(thinker.function, ActionFunc::None)
            });
            if status {
                // An item must always be replaced in place to prevent realloc of vec
                let mut thinker = self.thinkers[i].take().unwrap();
                thinker.unlink();
            }
        }
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
            if game.player_in_game[i] && player.think(level) {
                if let Some(ref mut mobj) = player.mobj {
                    mobj.unlink();
                    mobj.function = ActionFunc::None;
                }
            }
        }

        // P_RunThinkers ();, this may need to remove thinkers..
        // P_UpdateSpecials ();
        // P_RespawnSpecials ();

        // TODO: trial removal of mobs
        level.clean_thinker_list();

        level.level_time += 1;
    }
}
