use std::{cell::RefCell, rc::Rc};

use log::info;
use sound_sdl2::SndServerTx;
use sound_traits::{SfxEnum, SoundAction};
use wad::{lumps::WadThing, WadData};

use crate::{
    doom_def::{GameAction, GameMode, MAXPLAYERS, MAX_DEATHMATCH_STARTS},
    level::map_data::MapData,
    pic::Button,
    play::Skill,
    thinker::ThinkerAlloc,
    DPtr, PicData, Player,
};

use super::map_defs::LineDef;

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
    pub episode: i32,
    /// Required for the mobj controller (Boss check)
    pub game_map: i32,
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
    /// List of switch textures in ordered pairs
    pub switch_list: Vec<usize>,
    /// List of used buttons. Typically these buttons or switches are timed.
    pub button_list: Vec<Button>,
    pub line_special_list: Vec<DPtr<LineDef>>,
    /// Need access to texture data for a few things
    pub pic_data: Rc<RefCell<PicData>>,
    /// Some stuff needs to know the game mode (e.g, switching weapons)
    pub game_mode: GameMode,
    /// Provides ability for things to start a sound
    pub snd_command: SndServerTx,

    /// Tracks which players are currently active, set by d_net.c loop
    player_in_game: *const [bool; MAXPLAYERS],
    /// Each player in the array may be controlled
    players: *const [Player; MAXPLAYERS],
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
    pub unsafe fn new(
        skill: Skill,
        episode: i32,
        map: i32,
        game_mode: GameMode,
        switch_list: Vec<usize>,
        pic_data: Rc<RefCell<PicData>>,
        snd_command: SndServerTx,
        player_in_game: &[bool; MAXPLAYERS],
        players: &[Player; MAXPLAYERS],
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

        let map_data = MapData::new(map_name);

        // G_DoReborn
        // G_CheckSpot

        Level {
            map_data,
            thinkers: ThinkerAlloc::new(0),
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
            totalkills: 0,
            totalitems: 0,
            totalsecret: 0,
            game_action: None,
            secret_exit: false,
            valid_count: 0,
            switch_list,
            button_list: Vec::with_capacity(50),
            line_special_list: Vec::with_capacity(50),
            pic_data,
            game_mode,
            snd_command,
            player_in_game,
            players,
        }
    }

    pub fn player_in_game(&self) -> &[bool; MAXPLAYERS] {
        unsafe { &*self.player_in_game }
    }

    pub fn players(&self) -> &[Player; MAXPLAYERS] {
        unsafe { &*self.players }
    }

    pub fn load(&mut self, pic_data: &PicData, wad_data: &WadData) {
        self.map_data.load(pic_data, wad_data);
        unsafe {
            self.thinkers = ThinkerAlloc::new(self.map_data.get_things().len() + 500);
        }
    }

    pub fn do_exit_level(&mut self) {
        info!("Exited level");
        self.secret_exit = false;
        self.game_action = Some(GameAction::CompletedLevel);
    }

    pub fn do_secret_exit_level(&mut self) {
        info!("Secret exited level");
        self.secret_exit = true;
        self.game_action = Some(GameAction::CompletedLevel);
    }

    pub fn start_sound(&self, sfx: SfxEnum, x: f32, y: f32, uid: usize) {
        self.snd_command
            .send(SoundAction::StartSfx { uid, sfx, x, y })
            .unwrap();
    }
}
