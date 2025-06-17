//! Game state
//!
//! The state of the game can be a few states only:
//!
//! - level playing (gameplay crate)
//! - intermission/finale (separate machination crates)
//! - demo playing (runs a level usign gameplay crate)
//! - screen wipe
//!
//! The game state can be changed by a few actions - these are more concretely
//! defined as trait functions in `GameTraits`, where the exposed functions
//! trigger an action from `GameAction`. When an action is set it takes effect
//! on the next tic.
//!
//! Note that the primary state is either demo-play or level-play.
//!
//! The active game state also determines which `Machinations` are run, and the
//! order in which they run - these are such things as intermission screens or
//! statusbars during gameplay. In the case of a statusbar for example it ticks
//! only during the `GameState::Level` state, and draws to the buffer after the
//! player view is drawn.

pub mod game_impl;
pub mod subsystems;

use crate::subsystems::GameSubsystem;
use gameplay::log::{debug, error, info, trace, warn};
use gameplay::tic_cmd::{TIC_CMD_BUTTONS, TicCmd};
use gameplay::{
    GameAction, GameMission, GameMode, GameOptions, Level, MAXPLAYERS, MapObject, PicData, Player,
    PlayerState, STATES, Skill, StateNum, m_clear_random, respawn_specials, spawn_specials,
    update_specials,
};
use gamestate_traits::sdl2::AudioSubsystem;
use gamestate_traits::{GameState, GameTraits, SubsystemTrait, WorldInfo};
use sound_nosnd::SndServerTx;
use std::iter::Peekable;
use std::thread::JoinHandle;
use std::time::Duration;
use std::vec::IntoIter;
// use sound_sdl2::SndServerTx;
use sound_traits::{MusTrack, SoundAction, SoundServer, SoundServerTic};
use wad::WadData;
use wad::types::WadPatch;

pub const DEMO_MARKER: u8 = 0x80;
pub const BACKUPTICS: usize = 12;
/// Description of the unregistered shareware release
pub const DESC_SHAREWARE: &str = "DOOM Shareware";
/// Description of registered shareware release
pub const DESC_REGISTERED: &str = "DOOM Registered";
/// Description of The Ultimate Doom release
pub const DESC_ULTIMATE: &str = "The Ultimate DOOM";
/// Description of DOOM II commercial release
pub const DESC_COMMERCIAL: &str = "DOOM 2: Hell on Earth";

/// Data and details used for playback of demos
pub struct DemoData {
    /// Demo being played?
    playback: bool,
    /// Is in the overall demo loop? (titles, credits, demos)
    pub advance: bool,
    sequence: i8,
    buffer: Peekable<IntoIter<u8>>,
    name: String,
}

/// Details used for the demo screens (title, help, ordering)
pub struct PageData {
    pub name: &'static str,
    pub cache: WadPatch,
    page_tic: i32,
}

pub struct GameType {
    pub mode: GameMode,
    pub mission: GameMission,
    pub description: &'static str,
}

impl GameType {
    fn identify_version(wad: &WadData) -> Self {
        let mode;
        let mission;
        let description;

        if wad.lump_exists("MAP01") {
            mission = GameMission::Doom2;
        } else if wad.lump_exists("E1M1") {
            mission = GameMission::Doom;
        } else {
            panic!("Could not determine IWAD type");
        }

        if mission == GameMission::Doom {
            // Doom 1.  But which version?
            if wad.lump_exists("E4M1") {
                mode = GameMode::Retail;
                description = DESC_ULTIMATE;
            } else if wad.lump_exists("E3M1") {
                mode = GameMode::Registered;
                description = DESC_REGISTERED;
            } else {
                mode = GameMode::Shareware;
                description = DESC_SHAREWARE;
            }
        } else {
            mode = GameMode::Commercial;
            description = DESC_COMMERCIAL;
            // TODO: check for TNT or Plutonia
        }
        Self {
            mode,
            mission,
            description,
        }
    }
}

/// Game is very much driven by d_main, which operates as an orchestrator
pub struct Game {
    /// Contains the full wad file. Wads are tiny in terms of today's memory use
    /// so it doesn't hurt to store the full file in ram. May change later.
    pub wad_data: WadData,
    /// gametic at level start
    level_start_tic: u32,
    /// The complete `Level` data encompassing the everything everywhere all at
    /// once... (if loaded).
    pub level: Option<Level>,

    /// Data related to demo play and state
    pub demo: DemoData,
    /// The page currently shown during demo state
    pub page: PageData,
    /// Is the game running? Used as main loop control
    running: bool,

    /// Showing automap?
    automap: bool,
    /// only if started as net death

    /// player taking events and displaying
    pub consoleplayer: usize,
    /// view being displayed
    displayplayer: usize,
    /// Tracks which players are currently active, set by d_net.c loop
    pub players_in_game: [bool; MAXPLAYERS],
    /// Each player in the array may be controlled
    pub players: [Player; MAXPLAYERS],
    pub pic_data: PicData,

    //
    pending_action: GameAction,
    pub game_type: GameType,

    pub game_tic: u32,
    pub gamestate: GameState,
    /// If set to different from the `gamestate` then a wipe will be done.
    /// If `GameState::ForceWipe` is used then a wipe is always done - typically
    /// this is used during level changes as the gamestate here doesn't change.
    ///
    /// The state is picked up in `d_main`.
    pub wipe_game_state: GameState,

    /// If non-zero, exit the level after this number of minutes.
    _time_limit: Option<i32>,
    /// Intermission and world/map end data, used to show map and world stats,
    /// and queue up the next map or episode.
    world_info: WorldInfo,
    /// d_net.c
    pub netcmds: [[TicCmd; BACKUPTICS]; MAXPLAYERS],
    /// d_net.c
    _localcmds: [TicCmd; BACKUPTICS],
    usergame: bool,
    game_skill: Skill,
    pub paused: bool,

    /// The options the game-exe exe was started with
    pub options: GameOptions,
    /// Sound tx
    pub sound_cmd: SndServerTx,
    snd_thread: Option<JoinHandle<()>>,
}

impl Drop for Game {
    fn drop(&mut self) {
        self.sound_cmd.send(SoundAction::Shutdown).unwrap();
        let thread = self.snd_thread.take();
        thread.unwrap().join().unwrap();
        std::thread::sleep(Duration::from_millis(500));
    }
}

impl Game {
    pub fn new(
        mut options: GameOptions,
        mut wad: WadData,
        snd_ctx: AudioSubsystem,
        sfx_vol: i32,
        mus_vol: i32,
    ) -> Game {
        let game_type = GameType::identify_version(&wad);

        // make sure map + episode aren't 0 from CLI option block
        if options.map == 0 {
            options.map = 1;
        }
        if options.episode == 0 {
            options.episode = 1;
        }

        debug!("Game: new mode = {:?}", game_type.mode);
        if game_type.mode == GameMode::Retail {
            if options.episode > 4 && options.pwad.is_empty() {
                warn!(
                    "Game: new: {:?} mode (no pwad) but episode {} is greater than 4",
                    game_type.mode, options.episode
                );
                options.episode = 4;
            }
        } else if game_type.mode == GameMode::Shareware {
            if options.episode > 1 {
                warn!(
                    "Game: new: {:?} mode but episode {} is greater than 1",
                    game_type.mode, options.episode
                );
                options.episode = 1; // only start episode 1 on shareware
            }
            if options.map > 5 {
                warn!(
                    "Game: init_new: {:?} mode but map {} is greater than 5",
                    game_type.mode, options.map
                );
                options.map = 5;
            }
        } else if options.episode > 3 {
            warn!(
                "Game: new: {:?} mode but episode {} is greater than 3",
                game_type.mode, options.episode
            );
            options.episode = 3;
        }

        if options.map > 9 && game_type.mode != GameMode::Commercial {
            warn!(
                "Game: init_new: {:?} mode but map {} is greater than 9",
                game_type.mode, options.map
            );
            options.map = 9;
        }

        if !options.pwad.is_empty() {
            info!("Init PWADfiles");
            for pwad in options.pwad.iter() {
                wad.add_file(pwad.into());
                info!("Added: {}", pwad);
            }
        }

        // Mimic the OG output
        println!(
            "\nROOM-4-DOOM v{}. Playing {}",
            env!("CARGO_PKG_VERSION"),
            game_type.description,
        );

        match game_type.mode {
            GameMode::Shareware => {
                println!(
                    r#"
===========================================================================
                            Shareware WAD!
===========================================================================
"#
                );
            }
            _ => {
                println!(
                    r#"
===========================================================================
                 Commercial WAD - do not distribute!
===========================================================================
"#
                );
            }
        }

        info!("Init playloop state.");

        let snd_thread;
        let snd_tx = match sound_sdl2::Snd::new(snd_ctx, &wad) {
            Ok(mut s) => {
                let tx = s.init().unwrap();
                snd_thread = std::thread::spawn(move || {
                    loop {
                        if !s.tic() {
                            break;
                        }
                    }
                });
                tx.send(SoundAction::SfxVolume(sfx_vol)).unwrap();
                tx.send(SoundAction::MusicVolume(mus_vol)).unwrap();
                tx
            }
            Err(e) => {
                warn!("Could not set up sound server: {e}");
                let mut s = sound_nosnd::Snd::new(&wad).unwrap();
                let tx = s.init().unwrap();
                snd_thread = std::thread::spawn(move || {
                    loop {
                        if !s.tic() {
                            break;
                        }
                    }
                });
                tx
            }
        };

        // TODO: D_CheckNetGame ();
        // TODO: HU_Init ();
        // TODO: ST_Init ();

        let mut game_action = GameAction::None;
        if options.warp {
            game_action = GameAction::NewGame;
        }

        let lump = wad.get_lump("TITLEPIC").expect("TITLEPIC missing");
        let page_cache = WadPatch::from_lump(lump);
        let pic_data = PicData::init(false, &wad);

        Game {
            wad_data: wad,
            level_start_tic: 0,
            level: None,
            demo: DemoData {
                playback: false,
                buffer: Vec::new().into_iter().peekable(),
                name: String::new(),
                advance: false,
                sequence: 0,
            },
            page: PageData {
                name: "TITLEPIC",
                cache: page_cache,
                page_tic: 200,
            },
            pic_data,
            running: true,

            automap: false,
            consoleplayer: 0, // TODO: should be set in d_net.c

            displayplayer: 0,
            players_in_game: [false, false, false, false],
            players: [
                Player::default(),
                Player::default(),
                Player::default(),
                Player::default(),
            ],

            pending_action: game_action,

            game_type,

            game_tic: 0,
            // Start the display with a wipe. Looks cool
            gamestate: GameState::ForceWipe,
            // Initial state is changed later, here doesn't matter
            wipe_game_state: GameState::DemoScreen,
            _time_limit: None,
            world_info: WorldInfo::default(),

            netcmds: [[TicCmd::new(); BACKUPTICS]; MAXPLAYERS],
            _localcmds: [TicCmd::new(); BACKUPTICS],

            usergame: false,
            game_skill: Skill::default(),
            paused: false,
            options,
            sound_cmd: snd_tx,
            snd_thread: Some(snd_thread),
        }
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn set_running(&mut self, run: bool) {
        self.running = run;
    }

    pub fn is_netgame(&self) -> bool {
        self.options.netgame
    }

    pub fn game_skill(&self) -> Skill {
        self.game_skill
    }

    pub fn game_mission(&self) -> GameMission {
        self.game_type.mission
    }

    fn do_new_game(&mut self) {
        debug!("Entered do_new_game");

        self.options.respawn_monsters = matches!(self.options.skill, Skill::Nightmare);
        self.options.netgame = false;
        self.options.deathmatch = 0;
        self.players_in_game.fill(false);
        self.players_in_game[self.consoleplayer] = true;

        self.init_new();
        self.pending_action = GameAction::None;
    }

    fn init_new(&mut self) {
        debug!("Entered init_new");

        if self.paused {
            self.paused = false;
            // TODO: S_ResumeSound();
        }

        debug!("Game: init_new: mode = {:?}", self.game_type.mode);
        if self.game_type.mode == GameMode::Retail {
            if self.options.episode > 4 && self.options.pwad.is_empty() {
                warn!(
                    "Game: init_new: {:?} mode but episode {} is greater than 4",
                    self.game_type.mode, self.options.episode
                );
                self.options.episode = 4;
            }
        } else if self.game_type.mode == GameMode::Shareware {
            if self.options.episode > 1 {
                warn!(
                    "Game: init_new: {:?} mode but episode {} is greater than 1",
                    self.game_type.mode, self.options.episode
                );
                self.options.episode = 1; // only start episode 1 on shareware
            }
            if self.options.map > 5 {
                warn!(
                    "Game: init_new: {:?} mode but map {} is greater than 5",
                    self.game_type.mode, self.options.map
                );
                self.options.map = 5;
            }
        } else if self.options.episode > 3 && self.options.pwad.is_empty() {
            warn!(
                "Game: init_new: {:?} mode but episode {} is greater than 3",
                self.game_type.mode, self.options.episode
            );
            self.options.episode = 3;
        }

        if self.options.map > 9 && self.game_type.mode != GameMode::Commercial {
            warn!(
                "Game: init_new: {:?} mode but map {} is greater than 9",
                self.game_type.mode, self.options.map
            );
            self.options.map = 9;
        }

        m_clear_random();

        self.options.respawn_monsters =
            self.options.skill == Skill::Nightmare || self.options.respawn_parm;

        let game_skill = self.game_skill();
        let skill = self.options.skill;

        if skill == Skill::Nightmare && game_skill != Skill::Nightmare {
            for i in StateNum::SARG_RUN1 as usize..StateNum::SARG_PAIN2 as usize {
                unsafe {
                    STATES[i].tics >>= 1;
                }
            }
            // TODO: mut mobj info
            // mobjinfo[MT_BRUISERSHOT].speed = 20 * FRACUNIT;
            // mobjinfo[MT_HEADSHOT].speed = 20 * FRACUNIT;
            // mobjinfo[MT_TROOPSHOT].speed = 20 * FRACUNIT;
        } else if skill != Skill::Nightmare && game_skill == Skill::Nightmare {
            for i in StateNum::SARG_RUN1 as usize..StateNum::SARG_PAIN2 as usize {
                unsafe {
                    STATES[i].tics <<= 1;
                }
            }
            // mobjinfo[MT_BRUISERSHOT].speed = 15 * FRACUNIT;
            // mobjinfo[MT_HEADSHOT].speed = 10 * FRACUNIT;
            // mobjinfo[MT_TROOPSHOT].speed = 10 * FRACUNIT;
        }

        // force players to be initialized upon first level load
        for player in self.players.iter_mut() {
            player.player_state = PlayerState::Reborn;
        }

        self.game_skill = self.options.skill;
        self.paused = false;
        self.demo.playback = false;
        self.automap = false;
        self.usergame = true; // will be set false if a demo

        info!("Begin new game!");
        self.do_load_level();
    }

    /// Doom function name `G_DoLoadLevel`
    fn do_load_level(&mut self) {
        debug!("Entered do_load_level");
        if self.wipe_game_state == GameState::Level {
            self.wipe_game_state = GameState::ForceWipe;
        }
        self.gamestate = GameState::Level;

        for player in self.players.iter_mut() {
            if player.player_state == PlayerState::Dead {
                player.player_state = PlayerState::Reborn;
                for i in 0..player.frags.len() {
                    player.frags[i] = 0;
                }
            }
            // Player setup from P_SetupLevel
            player.total_kills = 0;
            player.secrets_found = 0;
            player.items_collected = 0;
        }

        self.displayplayer = self.consoleplayer; // view the guy you are playing

        // TODO: starttime = I_GetTime();
        self.pending_action = GameAction::None;

        // Verify and set the map number + name
        let map_name = if self.game_type.mode == GameMode::Commercial {
            if self.options.map < 10 {
                format!("MAP0{}", self.options.map)
            } else {
                format!("MAP{}", self.options.map)
            }
        } else {
            format!("E{}M{}", self.options.episode, self.options.map)
        };

        let level = unsafe {
            Level::new_empty(
                self.options.clone(),
                self.game_type.mode,
                self.sound_cmd.clone(),
                &self.players_in_game,
                &mut self.players,
            )
        };

        info!(
            "Level started: E{} M{}, skill: {:?}",
            level.options.episode, level.options.map, level.options.skill,
        );
        self.level = Some(level);

        if let Some(ref mut level) = self.level {
            level.load(
                &map_name,
                self.game_type.mode,
                &mut self.pic_data,
                &self.wad_data,
            );

            // Pointer stuff must be set up *AFTER* the level data has been allocated
            // (it moves when punted to Some<Level>)
            let thing_list = (*level.map_data.things()).to_owned();

            for thing in &thing_list {
                MapObject::p_spawn_map_thing(
                    *thing,
                    self.options.no_monsters,
                    level,
                    &mut self.players,
                    &self.players_in_game,
                );
            }
            spawn_specials(level);

            debug!("Level: skill = {:?}", &level.options.skill);
            debug!("Level: episode = {}", &level.options.episode);
            debug!("Level: map = {}", &level.options.map);
            debug!("Level: player_starts = {:?}", &level.player_starts);

            self.level_start_tic = self.game_tic;
        }

        // Player setup from P_SetupLevel
        self.world_info.maxfrags = 0;
        self.world_info.partime = 180;
        self.players[self.consoleplayer].viewz = 1.0;
        // TODO: remove after new-game-exe stuff done

        self.change_music(MusTrack::None);
    }

    fn do_reborn(&mut self, _player_num: usize) {
        info!("Player respawned");
        self.pending_action = GameAction::LoadLevel;
        // self.players[player_num].
        // TODO: deathmatch spawns
    }

    /// G_DoLoadGame
    fn do_load_game(&mut self) {
        todo!("do_load_game");
    }

    /// G_DoSaveGame
    fn do_save_game(&mut self) {
        todo!("do_save_game");
    }

    pub fn start_title(&mut self) {
        self.demo.sequence = -1;
        self.pending_action = GameAction::None;
        self.advance_demo();
    }

    fn check_demo_status(&mut self) -> bool {
        if self.demo.playback {
            self.demo.playback = false;
            self.options.netgame = false;
            self.options.deathmatch = 0;
            for p in self.players_in_game.iter_mut() {
                *p = false;
            }
            self.options.respawn_parm = false;
            self.options.fast_parm = false;
            self.options.no_monsters = false;
            self.consoleplayer = 0;

            self.advance_demo();

            return true;
        }
        false
    }

    /// G_ReadDemoTicCmd
    fn read_demo_tic_cmd(&mut self, cmd: &mut TicCmd) {
        if let Some(byte) = self.demo.buffer.peek() {
            if *byte == DEMO_MARKER {
                self.check_demo_status();
                return;
            }
        } else {
            self.check_demo_status();
            return;
        }

        if let Some(byte) = self.demo.buffer.next() {
            cmd.forwardmove = byte as i8;
        }
        if let Some(byte) = self.demo.buffer.next() {
            cmd.sidemove = byte as i8;
        }
        if let Some(byte) = self.demo.buffer.next() {
            cmd.angleturn = (byte as i16) << 8;
        }
        if let Some(byte) = self.demo.buffer.next() {
            cmd.buttons = byte;
        }
    }

    pub fn advance_demo(&mut self) {
        self.demo.advance = true;
    }

    /// D_PageTicker();
    fn page_ticker(&mut self) {
        self.page.page_tic -= 1;
        if self.page.page_tic < 0 {
            self.advance_demo();
        }
    }

    pub fn do_advance_demo(&mut self) {
        self.players[self.consoleplayer].player_state = PlayerState::Live;
        self.demo.advance = false;
        self.usergame = false;
        self.paused = false;
        self.pending_action = GameAction::None;

        if self.game_type.mode == GameMode::Retail {
            self.demo.sequence = (self.demo.sequence + 1) % 7;
        } else {
            self.demo.sequence = (self.demo.sequence + 1) % 6;
        }

        if !self.options.enable_demos {
            if matches!(self.demo.sequence, 1 | 3 | 5 | 6) {
                self.demo.sequence += 1;
            }
            if self.demo.sequence > 4 {
                self.demo.sequence = 0;
            }
        }

        match self.demo.sequence {
            0 => {
                if self.game_type.mode == GameMode::Commercial {
                    self.page.page_tic = 35 * 11;
                } else {
                    self.page.page_tic = 170;
                }
                self.gamestate = GameState::DemoScreen;
                self.page.name = "TITLEPIC";

                if self.game_type.mode == GameMode::Commercial {
                    self.sound_cmd
                        .send(SoundAction::ChangeMusic(MusTrack::Dm2ttl as usize, false))
                        .expect("Title music failed");
                } else {
                    self.sound_cmd
                        .send(SoundAction::ChangeMusic(MusTrack::Intro as usize, false))
                        .expect("Title music failed");
                }
            }
            1 => self.defered_play_demo("demo1".into()),
            2 => {
                self.page.page_tic = 200;
                self.gamestate = GameState::DemoScreen;
                self.page.name = "CREDIT";
            }
            3 => self.defered_play_demo("demo2".into()),
            4 => {
                self.gamestate = GameState::DemoScreen;
                if self.game_type.mode == GameMode::Commercial {
                    self.page.page_tic = 35 * 11;
                    self.sound_cmd
                        .send(SoundAction::ChangeMusic(MusTrack::Dm2ttl as usize, false))
                        .expect("Title music failed");
                    self.page.name = "TITLEPIC";
                } else {
                    self.page.page_tic = 200;
                    if self.game_type.mode == GameMode::Retail {
                        self.page.name = "CREDIT";
                    } else {
                        self.page.name = "HELP2";
                    }
                }
            }
            5 => self.defered_play_demo("demo3".into()),
            6 => self.defered_play_demo("demo4".into()),
            _ => {}
        }
    }

    /// G_DeferedPlayDemo
    fn defered_play_demo(&mut self, name: String) {
        self.demo.name = name;
        self.pending_action = GameAction::PlayDemo;
    }

    /// G_DoPlayDemo
    fn do_play_demo(&mut self) {
        self.pending_action = GameAction::None;

        if let Some(demo) = self.wad_data.get_lump(&self.demo.name) {
            self.demo.buffer = demo.data.clone().into_iter().peekable();

            if let Some(byte) = self.demo.buffer.next() {
                if byte != 109 {
                    self.pending_action = GameAction::None;
                    return;
                }
            }

            if let Some(byte) = self.demo.buffer.next() {
                self.options.skill = Skill::from(byte);
            }
            if let Some(byte) = self.demo.buffer.next() {
                self.options.episode = byte as usize;
            }
            if let Some(byte) = self.demo.buffer.next() {
                self.options.map = byte as usize;
            }
            if let Some(byte) = self.demo.buffer.next() {
                self.options.deathmatch = byte;
            }
            if let Some(byte) = self.demo.buffer.next() {
                self.options.respawn_parm = byte == 1;
            }
            if let Some(byte) = self.demo.buffer.next() {
                self.options.fast_parm = byte == 1;
            }
            if let Some(byte) = self.demo.buffer.next() {
                self.options.no_monsters = byte == 1;
            }
            if let Some(byte) = self.demo.buffer.next() {
                self.consoleplayer = byte as usize;
            }
            for player in self.players_in_game.iter_mut() {
                if let Some(byte) = self.demo.buffer.next() {
                    *player = byte == 1;
                }
            }
            if self.players_in_game[1] {
                // TODO: netgame stuff
            }

            self.init_new();
            self.usergame = false;
            self.demo.playback = true;
        } else {
            error!("Demo {} does not exist", self.demo.name);
            self.pending_action = GameAction::None;
        }
    }

    /// Load the next level and set the `GameAction` to None
    ///
    /// Doom function name `G_DoWorldDone`
    fn do_world_done(&mut self) {
        self.options.map = self.world_info.next + 1;
        self.do_load_level();
        self.gamestate = GameState::Level;
        self.pending_action = GameAction::None;
        // TODO: viewactive = true;
    }

    /// Cleanup, re-init, and set up for next level or episode. Also sets up
    /// info that can be displayed on the intermission screene.
    fn do_completed(&mut self) {
        self.pending_action = GameAction::None;

        for (i, in_game) in self.players_in_game.iter().enumerate() {
            if *in_game {
                let player = &mut self.players[i];
                player.finish_level();
            }
        }

        self.world_info.didsecret = self.players[self.consoleplayer].didsecret;
        self.world_info.episode = self.options.episode - 1;
        self.world_info.last = self.options.map;

        if !matches!(self.game_type.mode, GameMode::Commercial) {
            if self.options.map == 8 {
                self.pending_action = GameAction::Victory;
                return;
            }
            if self.options.map == 8 {
                for p in self.players.iter_mut() {
                    p.didsecret = true;
                }
            }
        }

        // wminfo.next is 0 biased, unlike gamemap, which is just bloody confusing...
        if matches!(self.game_type.mode, GameMode::Commercial) {
            if self.level.as_ref().unwrap().secret_exit {
                if self.options.map == 15 {
                    self.world_info.next = 30;
                } else if self.options.map == 31 {
                    self.world_info.next = 31;
                }
            } else if self.options.map == 31 || self.options.map == 32 {
                self.world_info.next = 15;
            } else {
                self.world_info.next = self.options.map;
            }
        } else if self.level.as_ref().unwrap().secret_exit {
            // go to secret level
            self.world_info.next = 8;
        } else if self.options.map == 9 {
            match self.options.episode {
                1 => self.world_info.next = 3,
                2 => self.world_info.next = 5,
                3 => self.world_info.next = 6,
                4 => self.world_info.next = 2,
                _ => {}
            }
        } else {
            self.world_info.next = self.options.map;
        }

        self.world_info.maxkills = self.level.as_ref().unwrap().total_level_kills;
        self.world_info.maxitems = self.level.as_ref().unwrap().total_level_items;
        self.world_info.maxsecret = self.level.as_ref().unwrap().total_level_secrets;
        self.world_info.maxfrags = 0;

        // TODO: par times

        for (i, in_game) in self.players_in_game.iter().enumerate() {
            self.world_info.plyr[i].inn = *in_game;
            self.world_info.plyr[i].total_kills = self.players[i].total_kills;
            self.world_info.plyr[i].items_collected = self.players[i].items_collected;
            self.world_info.plyr[i].secrets_found = self.players[i].secrets_found;
            self.world_info.plyr[i].level_time = if let Some(level) = &self.level {
                level.level_time
            } else {
                0
            };
            self.world_info.plyr[i]
                .frags
                .copy_from_slice(&self.players[i].frags);
        }

        self.level = None; // Drop level data
        self.gamestate = GameState::Intermission;
    }

    fn start_finale(&mut self) {
        self.world_info.didsecret = self.players[self.consoleplayer].didsecret;
        self.world_info.episode = self.options.episode;
        self.world_info.last = self.options.map;

        self.gamestate = GameState::Finale;
        self.level = None; // drop the level
        self.pending_action = GameAction::None;
    }

    /// The ticker which controls the state the game-exe is in. For example the
    /// game-exe could be in menu mode, demo play, intermission
    /// (`GameState`). A state may also be running other functions that can
    /// change the game-exe state or cause an action through `GameAction`.
    ///
    /// Doom function name `G_Ticker`
    pub fn ticker<I, S, H, F>(&mut self, machinations: &mut GameSubsystem<I, S, H, F>)
    where
        I: SubsystemTrait,
        S: SubsystemTrait,
        H: SubsystemTrait,
        F: SubsystemTrait,
    {
        trace!("Entered ticker");
        // do player reborns if needed
        for i in 0..MAXPLAYERS {
            if self.players_in_game[i] && self.players[i].player_state == PlayerState::Reborn {
                self.do_reborn(i);
            }
        }

        if let Some(level) = &mut self.level {
            if let Some(action) = level.game_action.take() {
                self.pending_action = action;
                info!("Game state changed: {:?}", self.pending_action);
            }
        }

        // do things to change the game-exe state
        match self.pending_action {
            GameAction::LoadLevel => {
                machinations.hud_msgs.init(self);
                self.do_load_level();
            }
            GameAction::NewGame => self.do_new_game(),
            GameAction::CompletedLevel => {
                self.do_completed();
                machinations.intermission.init(self);
                machinations.hud_msgs.init(self);
            }
            GameAction::None => {}
            GameAction::LoadGame => self.do_load_game(),
            GameAction::SaveGame => self.do_save_game(),
            GameAction::PlayDemo => self.do_play_demo(),
            GameAction::Victory => {
                self.start_finale();
                machinations.finale.init(self);
                machinations.hud_msgs.init(self);
            }
            GameAction::WorldDone => self.do_world_done(),
            GameAction::Screenshot => todo!("M_ScreenShot(); gameaction = ga_nothing"),
        }

        // TODO: get commands, check consistancy,
        // and build new consistancy check
        // buf = (gametic / ticdup) % BACKUPTICS;

        // Checks ticcmd consistency and turbo cheat
        for i in 0..MAXPLAYERS {
            if self.players_in_game[i] {
                // sets the players cmd for this tic
                self.players[i].cmd = self.netcmds[i][0];
                // memcpy(cmd, &netcmds[i][buf], sizeof(ticcmd_t));
                if self.demo.playback {
                    let mut cmd = self.players[i].cmd;
                    self.read_demo_tic_cmd(&mut cmd);
                    self.players[i].cmd = cmd;
                }
                // if (demorecording)
                //     TODO: G_WriteDemoTiccmd(cmd);
                // TODO: Netgame stuff here
            }
        }

        // check for special buttons
        for i in 0..MAXPLAYERS {
            #[allow(clippy::if_same_then_else)]
            if self.players_in_game[i]
                && self.players[i].cmd.buttons & TIC_CMD_BUTTONS.bt_special > 0
            {
                let mask = self.players[i].cmd.buttons & TIC_CMD_BUTTONS.bt_specialmask;
                if mask == TIC_CMD_BUTTONS.bt_specialmask {
                    //     paused ^= 1;
                    //     if (paused)
                    //         S_PauseSound();
                    //     else
                    //         S_ResumeSound();
                    //     break;
                } else if mask == TIC_CMD_BUTTONS.bts_savegame {
                    //     if (!savedescription[0])
                    //         strcpy(savedescription, "NET GAME");
                    //     savegameslot =
                    //         (players[i].cmd.buttons & BTS_SAVEMASK) >>
                    // BTS_SAVESHIFT;     gameaction =
                    // ga_savegame;     break;
                }
            }
        }

        match self.gamestate {
            GameState::Level => {
                // player movements, run thinkers etc
                self.p_ticker();
                // update statusbar information
                machinations.statusbar.ticker(self);
                // update the automap display info
                // AM_Ticker();
                // update the HUD statuses (things like timeout displayed messages)
                machinations.hud_msgs.ticker(self);
            }
            GameState::Intermission => {
                // WI_Ticker calls world_done()
                machinations.intermission.ticker(self);
            }
            GameState::Finale => {
                machinations.finale.ticker(self);
            }
            GameState::DemoScreen => {
                self.page_ticker();
            }
            GameState::ForceWipe => {
                // do a wipe
            }
        }
    }

    /// Gameplay ticker. Updates the game-exe level state along with all
    /// thinkers inside that level. Also watches for `TicCmd` that initiate
    /// another action or state such as pausing in menus, demo recording,
    /// save/load.
    ///
    /// Doom function name `P_Ticker`
    fn p_ticker(&mut self) {
        if self.paused {
            return;
        }
        // TODO: pause if in menu and at least one tic has been run
        // if ( !netgame
        //     && menuactive
        //     && !demoplayback
        // if game-exe.players[game-exe.consoleplayer].viewz as i32 != 1 {
        //     return;
        // }

        // Only run thinkers if a level is loaded

        if let Some(ref mut level) = self.level {
            for (i, player) in self.players.iter_mut().enumerate() {
                if self.players_in_game[i] && !player.think(level) {
                    // TODO: what to do with dead player?
                }
            }

            unsafe {
                let lev = &mut *(level as *mut Level);
                level.thinkers.run_thinkers(lev);
            }

            level.level_time += 1;

            update_specials(level, &mut self.pic_data);
            respawn_specials(level);
        }
    }
}
