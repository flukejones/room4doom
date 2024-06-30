//! Game state, fairly self-descriptive but bares expanding on in a little more
//! detail.
//!
//! The state of the game can be a few states only:
//!
//! - level playing
//! - intermission/finale
//! - demo playing
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
pub mod machination;

use crate::machination::Machinations;
use gameplay::log::{debug, error, info, trace, warn};
use gameplay::tic_cmd::{TicCmd, TIC_CMD_BUTTONS};
use gameplay::{
    log, m_clear_random, respawn_specials, spawn_specials, update_specials, GameAction,
    GameMission, GameMode, Level, MapObject, PicAnimation, PicData, Player, PlayerState, Skill,
    Switches, WBStartStruct, MAXPLAYERS,
};
use gamestate_traits::sdl2::AudioSubsystem;
use gamestate_traits::{GameState, GameTraits, MachinationTrait};
use sound_nosnd::SndServerTx;
use std::cell::RefCell;
use std::iter::Peekable;
use std::rc::Rc;
use std::time::Duration;
use std::vec::IntoIter;
// use sound_sdl2::SndServerTx;
use sound_traits::{MusTrack, SoundAction, SoundServer, SoundServerTic};
use wad::types::WadPatch;
use wad::WadData;

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

/// Options specific to Doom gameplay
pub struct DoomOptions {
    pub iwad: String,
    pub pwad: Vec<String>,
    pub no_monsters: bool,
    pub respawn_parm: bool,
    pub fast_parm: bool,
    pub dev_parm: bool,
    pub deathmatch: u8,
    pub skill: Skill,
    pub warp: bool,
    pub episode: usize,
    pub map: usize,
    pub autostart: bool,
    pub hi_res: bool,
    pub verbose: log::LevelFilter,
}

impl Default for DoomOptions {
    fn default() -> Self {
        Self {
            iwad: "doom.wad".to_string(),
            pwad: Default::default(),
            no_monsters: Default::default(),
            respawn_parm: Default::default(),
            fast_parm: Default::default(),
            dev_parm: Default::default(),
            deathmatch: Default::default(),
            skill: Default::default(),
            episode: Default::default(),
            map: Default::default(),
            warp: false,
            autostart: Default::default(),
            hi_res: true,
            verbose: log::LevelFilter::Info,
        }
    }
}

fn identify_version(wad: &WadData) -> (GameMode, GameMission, &'static str) {
    let game_mode;
    let game_mission;
    let game_description;

    if wad.lump_exists("MAP01") {
        game_mission = GameMission::Doom2;
    } else if wad.lump_exists("E1M1") {
        game_mission = GameMission::Doom;
    } else {
        panic!("Could not determine IWAD type");
    }

    if game_mission == GameMission::Doom {
        // Doom 1.  But which version?
        if wad.lump_exists("E4M1") {
            game_mode = GameMode::Retail;
            game_description = DESC_ULTIMATE;
        } else if wad.lump_exists("E3M1") {
            game_mode = GameMode::Registered;
            game_description = DESC_REGISTERED;
        } else {
            game_mode = GameMode::Shareware;
            game_description = DESC_SHAREWARE;
        }
    } else {
        game_mode = GameMode::Commercial;
        game_description = DESC_COMMERCIAL;
        // TODO: check for TNT or Plutonia
    }
    (game_mode, game_mission, game_description)
}

/// Game is very much driven by d_main, which operates as an orchestrator
pub struct Game {
    pub page_cache: WadPatch,
    /// Contains the full wad file. Wads are tiny in terms of today's memory use
    /// so it doesn't hurt to store the full file in ram. May change later.
    pub wad_data: WadData,
    /// The complete `Level` data encompassing the everything everywhere all at
    /// once... (if loaded).
    pub level: Option<Level>,
    /// Pre-composed textures, shared to the renderer. `doom-lib` owns and uses
    /// access to change animations + translation tables.
    pub pic_data: Rc<RefCell<PicData>>,
    /// Pre-generated texture animations
    pub animations: Vec<PicAnimation>,
    /// List of switch textures in ordered pairs
    pub switch_list: Vec<usize>,
    /// Is the game running? Used as main loop control
    running: bool,
    /// Demo being played?
    demo_playback: bool,
    /// Is in the overall demo loop? (titles, credits, demos)
    pub demo_advance: bool,
    demo_sequence: i8,
    demo_buffer: Peekable<IntoIter<u8>>,
    demo_name: String,
    pub page_name: &'static str,
    page_tic: i32,

    /// Showing automap?
    automap: bool,
    /// only if started as net death
    deathmatch: bool,
    /// only true if packets are broadcast
    netgame: bool,

    /// Tracks which players are currently active, set by d_net.c loop
    pub player_in_game: [bool; MAXPLAYERS],
    /// Each player in the array may be controlled
    pub players: [Player; MAXPLAYERS],

    //
    old_game_state: GameState,
    game_action: GameAction,
    pub gamestate: GameState,
    game_skill: Skill,
    respawn_monsters: bool,
    game_episode: usize,
    game_map: usize,
    pub game_tic: u32,

    /// If non-zero, exit the level after this number of minutes.
    _time_limit: Option<i32>,

    pub paused: bool,

    /// player taking events and displaying
    pub consoleplayer: usize,
    /// view being displayed
    displayplayer: usize,
    /// gametic at level start
    level_start_tic: u32,

    wminfo: WBStartStruct,
    /// d_net.c
    pub netcmds: [[TicCmd; BACKUPTICS]; MAXPLAYERS],
    /// d_net.c
    _localcmds: [TicCmd; BACKUPTICS],

    pub game_mode: GameMode,
    game_mission: GameMission,
    pub wipe_game_state: GameState,
    usergame: bool,

    /// The options the game-exe exe was started with
    pub options: DoomOptions,

    /// Sound tx
    pub snd_command: SndServerTx,
}

impl Drop for Game {
    fn drop(&mut self) {
        self.snd_command.send(SoundAction::Shutdown).unwrap();
        // Nightly only
        //while self.snd_thread.is_running() {}
        std::thread::sleep(Duration::from_millis(500));
    }
}

impl Game {
    pub fn new(
        mut options: DoomOptions,
        mut wad: WadData,
        snd_ctx: AudioSubsystem,
        sfx_vol: i32,
        mus_vol: i32,
    ) -> Game {
        // TODO: a bunch of version checks here to determine what game-exe mode
        let respawn_monsters = matches!(options.skill, Skill::Nightmare);

        let (game_mode, game_mission, game_description) = identify_version(&wad);

        // make sure map + episode aren't 0 from CLI option block
        if options.map == 0 {
            options.map = 1;
        }
        if options.episode == 0 {
            options.episode = 1;
        }

        debug!("Game: new mode = {:?}", game_mode);
        if game_mode == GameMode::Retail {
            if options.episode > 4 && options.pwad.is_empty() {
                warn!(
                    "Game: new: {:?} mode (no pwad) but episode {} is greater than 4",
                    game_mode, options.episode
                );
                options.episode = 4;
            }
        } else if game_mode == GameMode::Shareware {
            if options.episode > 1 {
                warn!(
                    "Game: new: {:?} mode but episode {} is greater than 1",
                    game_mode, options.episode
                );
                options.episode = 1; // only start episode 1 on shareware
            }
            if options.map > 5 {
                warn!(
                    "Game: init_new: {:?} mode but map {} is greater than 5",
                    game_mode, options.map
                );
                options.map = 5;
            }
        } else if options.episode > 3 {
            warn!(
                "Game: new: {:?} mode but episode {} is greater than 3",
                game_mode, options.episode
            );
            options.episode = 3;
        }

        if options.map > 9 && game_mode != GameMode::Commercial {
            warn!(
                "Game: init_new: {:?} mode but map {} is greater than 9",
                game_mode, options.map
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
            game_description,
        );

        match game_mode {
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

        let pic_data = PicData::init(options.hi_res, &wad);
        info!("Init playloop state.");
        let animations = PicAnimation::init(&pic_data);
        let switch_list = Switches::init(game_mode, &pic_data);

        let tx = match sound_sdl2::Snd::new(snd_ctx, &wad) {
            Ok(mut s) => {
                let tx = s.init().unwrap();
                std::thread::spawn(move || loop {
                    if !s.tic() {
                        break;
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
                std::thread::spawn(move || loop {
                    if !s.tic() {
                        break;
                    }
                });
                tx
            }
        };

        // TODO: D_CheckNetGame ();
        // TODO: HU_Init ();
        // TODO: ST_Init ();

        let mut game_action = GameAction::None;
        let gamestate = GameState::DemoScreen;
        if options.warp {
            game_action = GameAction::NewGame;
        }

        let lump = wad.get_lump("TITLEPIC").expect("TITLEPIC missing");
        let page_cache = WadPatch::from_lump(lump);

        Game {
            page_cache,
            wad_data: wad,
            level: None,
            running: true,
            automap: false,
            demo_playback: false,
            demo_buffer: Vec::new().into_iter().peekable(),
            demo_name: String::new(),
            demo_advance: false,
            demo_sequence: 0,
            page_name: "TITLEPIC",
            page_tic: 200,

            pic_data: Rc::new(RefCell::new(pic_data)),
            animations,
            switch_list,

            players: [
                Player::default(),
                Player::default(),
                Player::default(),
                Player::default(),
            ],
            player_in_game: [false, false, false, false], // TODO: should be set in d_net.c

            paused: false,
            deathmatch: false,
            netgame: false,
            old_game_state: gamestate,
            game_action, // TODO: default to ga_nothing when more state is done
            gamestate,
            game_skill: options.skill,
            game_tic: 0,
            respawn_monsters,
            game_episode: options.episode,
            game_map: options.map,
            _time_limit: None,
            consoleplayer: 0,
            displayplayer: 0,
            level_start_tic: 0,
            wminfo: WBStartStruct::default(),

            netcmds: [[TicCmd::new(); BACKUPTICS]; MAXPLAYERS],
            _localcmds: [TicCmd::new(); BACKUPTICS],

            game_mode,
            game_mission,
            wipe_game_state: GameState::DemoScreen,
            usergame: false,
            options,
            snd_command: tx,
        }
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn set_running(&mut self, run: bool) {
        self.running = run;
    }

    pub fn is_netgame(&self) -> bool {
        self.netgame
    }

    pub fn game_skill(&self) -> Skill {
        self.game_skill
    }

    pub fn game_mission(&self) -> GameMission {
        self.game_mission
    }

    fn do_new_game(&mut self) {
        debug!("Entered do_new_game");

        self.netgame = false;
        self.deathmatch = false;
        for i in 0..self.players.len() {
            self.player_in_game[i] = false;
        }
        self.respawn_monsters = matches!(self.game_skill, Skill::Nightmare);
        self.consoleplayer = 0;
        self.player_in_game[0] = true;

        self.init_new();
        self.game_action = GameAction::None;
    }

    fn init_new(&mut self) {
        debug!("Entered init_new");

        if self.paused {
            self.paused = false;
            // TODO: S_ResumeSound();
        }

        debug!("Game: init_new: mode = {:?}", self.game_mode);
        if self.game_mode == GameMode::Retail {
            if self.game_episode > 4 && self.options.pwad.is_empty() {
                warn!(
                    "Game: init_new: {:?} mode but episode {} is greater than 4",
                    self.game_mode, self.game_episode
                );
                self.game_episode = 4;
            }
        } else if self.game_mode == GameMode::Shareware {
            if self.game_episode > 1 {
                warn!(
                    "Game: init_new: {:?} mode but episode {} is greater than 1",
                    self.game_mode, self.game_episode
                );
                self.game_episode = 1; // only start episode 1 on shareware
            }
            if self.game_map > 5 {
                warn!(
                    "Game: init_new: {:?} mode but map {} is greater than 5",
                    self.game_mode, self.game_map
                );
                self.game_map = 5;
            }
        } else if self.game_episode > 3 && self.options.pwad.is_empty() {
            warn!(
                "Game: init_new: {:?} mode but episode {} is greater than 3",
                self.game_mode, self.game_episode
            );
            self.game_episode = 3;
        }

        if self.game_map > 9 && self.game_mode != GameMode::Commercial {
            warn!(
                "Game: init_new: {:?} mode but map {} is greater than 9",
                self.game_mode, self.game_map
            );
            self.game_map = 9;
        }

        m_clear_random();

        self.respawn_monsters = self.game_skill == Skill::Nightmare || self.options.respawn_parm;

        // TODO: This shit (mobjinfo) is constant for now. Change it later
        // if (fastparm || (skill == sk_nightmare && gameskill != sk_nightmare))
        // {
        //     for (i = S_SARG_RUN1; i <= S_SARG_PAIN2; i++)
        //         states[i].tics >>= 1;
        //     mobjinfo[MT_BRUISERSHOT].speed = 20 * FRACUNIT;
        //     mobjinfo[MT_HEADSHOT].speed = 20 * FRACUNIT;
        //     mobjinfo[MT_TROOPSHOT].speed = 20 * FRACUNIT;
        // }
        // else if (skill != sk_nightmare && gameskill == sk_nightmare)
        // {
        //     for (i = S_SARG_RUN1; i <= S_SARG_PAIN2; i++)
        //         states[i].tics <<= 1;
        //     mobjinfo[MT_BRUISERSHOT].speed = 15 * FRACUNIT;
        //     mobjinfo[MT_HEADSHOT].speed = 10 * FRACUNIT;
        //     mobjinfo[MT_TROOPSHOT].speed = 10 * FRACUNIT;
        // }

        // force players to be initialized upon first level load
        for player in self.players.iter_mut() {
            player.player_state = PlayerState::Reborn;
        }

        self.paused = false;
        self.demo_playback = false;
        self.automap = false;
        self.usergame = true; // will be set false if a demo

        self.pic_data
            .borrow_mut()
            .set_sky_pic(self.game_mode, self.game_episode, self.game_map);

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
        self.game_action = GameAction::None;

        let level = unsafe {
            Level::new(
                self.game_skill,
                self.game_episode,
                self.game_map,
                self.game_mode,
                self.switch_list.clone(),
                self.pic_data.clone(),
                self.snd_command.clone(),
                &self.player_in_game,
                &mut self.players,
                self.pic_data.borrow().sky_num(),
            )
        };

        info!("Level started: E{} M{}", level.episode, level.game_map);
        self.level = Some(level);

        if let Some(ref mut level) = self.level {
            level.load(&self.wad_data);

            // Pointer stuff must be set up *AFTER* the level data has been allocated
            // (it moves when punted to Some<Level>)
            let thing_list = (*level.map_data.things()).to_owned();

            for thing in &thing_list {
                MapObject::p_spawn_map_thing(
                    *thing,
                    self.options.no_monsters,
                    level,
                    &mut self.players,
                    &self.player_in_game,
                );
            }
            spawn_specials(level);

            debug!("Level: skill = {:?}", &level.game_skill);
            debug!("Level: episode = {}", &level.episode);
            debug!("Level: map = {}", &level.game_map);
            debug!("Level: player_starts = {:?}", &level.player_starts);

            level.game_tic = self.game_tic;
            self.level_start_tic = self.game_tic;
            level.game_tic = self.game_tic;
        }

        // Player setup from P_SetupLevel
        self.wminfo.maxfrags = 0;
        self.wminfo.partime = 180;
        self.players[self.consoleplayer].viewz = 1.0;
        // TODO: remove after new-game-exe stuff done
        self.pic_data
            .borrow_mut()
            .set_sky_pic(self.game_mode, self.game_episode, self.game_map);

        self.change_music(MusTrack::None);
    }

    fn do_reborn(&mut self, _player_num: usize) {
        info!("Player respawned");
        self.game_action = GameAction::LoadLevel;
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
        self.demo_sequence = -1;
        self.game_action = GameAction::None;
        self.advance_demo();
    }

    fn check_demo_status(&mut self) -> bool {
        if self.demo_playback {
            self.demo_playback = false;
            self.netgame = false;
            self.deathmatch = false;
            for p in self.player_in_game.iter_mut() {
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
        if let Some(byte) = self.demo_buffer.peek() {
            if *byte == DEMO_MARKER {
                self.check_demo_status();
                return;
            }
        } else {
            self.check_demo_status();
            return;
        }

        if let Some(byte) = self.demo_buffer.next() {
            cmd.forwardmove = byte as i8;
        }
        if let Some(byte) = self.demo_buffer.next() {
            cmd.sidemove = byte as i8;
        }
        if let Some(byte) = self.demo_buffer.next() {
            cmd.angleturn = (byte as i16) << 8;
        }
        if let Some(byte) = self.demo_buffer.next() {
            cmd.buttons = byte;
        }
    }

    pub fn advance_demo(&mut self) {
        self.demo_advance = true;
    }

    /// D_PageTicker();
    fn page_ticker(&mut self) {
        self.page_tic -= 1;
        if self.page_tic < 0 {
            self.advance_demo();
        }
    }

    pub fn do_advance_demo(&mut self) {
        self.players[self.consoleplayer].player_state = PlayerState::Live;
        self.demo_advance = false;
        self.usergame = false;
        self.paused = false;
        self.game_action = GameAction::None;

        if self.game_mode == GameMode::Retail {
            self.demo_sequence = (self.demo_sequence + 1) % 7;
        } else {
            self.demo_sequence = (self.demo_sequence + 1) % 6;
        }

        dbg!(self.demo_sequence);
        match self.demo_sequence {
            0 => {
                if self.game_mode == GameMode::Commercial {
                    self.page_tic = 35 * 11;
                } else {
                    self.page_tic = 170;
                }
                self.gamestate = GameState::DemoScreen;
                self.page_name = "TITLEPIC";

                if self.game_mode == GameMode::Commercial {
                    self.snd_command
                        .send(SoundAction::ChangeMusic(MusTrack::Dm2ttl as usize, false))
                        .expect("Title music failed");
                } else {
                    self.snd_command
                        .send(SoundAction::ChangeMusic(MusTrack::Intro as usize, false))
                        .expect("Title music failed");
                }
            }
            1 => self.defered_play_demo("demo1".into()),
            2 => {
                self.page_tic = 200;
                self.gamestate = GameState::DemoScreen;
                self.page_name = "CREDIT";
            }
            3 => self.defered_play_demo("demo2".into()),
            4 => {
                self.gamestate = GameState::DemoScreen;
                if self.game_mode == GameMode::Commercial {
                    self.page_tic = 35 * 11;
                    self.snd_command
                        .send(SoundAction::ChangeMusic(MusTrack::Dm2ttl as usize, false))
                        .expect("Title music failed");
                    self.page_name = "TITLEPIC";
                } else {
                    self.page_tic = 200;
                    if self.game_mode == GameMode::Retail {
                        self.page_name = "CREDIT";
                    } else {
                        self.page_name = "HELP2";
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
        self.demo_name = name;
        self.game_action = GameAction::PlayDemo;
    }

    /// G_DoPlayDemo
    fn do_play_demo(&mut self) {
        self.game_action = GameAction::None;

        if let Some(demo) = self.wad_data.get_lump(&self.demo_name) {
            self.demo_buffer = demo.data.clone().into_iter().peekable();

            if let Some(byte) = self.demo_buffer.next() {
                if byte != 109 {
                    self.game_action = GameAction::None;
                    return;
                }
            }

            if let Some(byte) = self.demo_buffer.next() {
                self.game_skill = Skill::from(byte);
            }
            if let Some(byte) = self.demo_buffer.next() {
                self.game_episode = byte as usize;
            }
            if let Some(byte) = self.demo_buffer.next() {
                self.game_map = byte as usize;
            }
            if let Some(byte) = self.demo_buffer.next() {
                self.deathmatch = byte == 1;
            }
            if let Some(byte) = self.demo_buffer.next() {
                self.options.respawn_parm = byte == 1;
            }
            if let Some(byte) = self.demo_buffer.next() {
                self.options.fast_parm = byte == 1;
            }
            if let Some(byte) = self.demo_buffer.next() {
                self.options.no_monsters = byte == 1;
            }
            if let Some(byte) = self.demo_buffer.next() {
                self.consoleplayer = byte as usize;
            }
            for player in self.player_in_game.iter_mut() {
                if let Some(byte) = self.demo_buffer.next() {
                    *player = byte == 1;
                }
            }
            if self.player_in_game[1] {
                // TODO: netgame stuff
            }

            self.init_new();
            self.usergame = false;
            self.demo_playback = true;
        } else {
            error!("Demo {} does not exist", self.demo_name);
            self.game_action = GameAction::None;
        }
    }

    /// Load the next level and set the `GameAction` to None
    ///
    /// Doom function name `G_DoWorldDone`
    fn do_world_done(&mut self) {
        self.game_map = self.wminfo.next + 1;
        self.do_load_level();
        self.gamestate = GameState::Level;
        self.game_action = GameAction::None;
        // TODO: viewactive = true;
    }

    /// Cleanup, re-init, and set up for next level or episode. Also sets up
    /// info that can be displayed on the intermission screene.
    fn do_completed(&mut self) {
        self.game_action = GameAction::None;

        for (i, in_game) in self.player_in_game.iter().enumerate() {
            if *in_game {
                let player = &mut self.players[i];
                player.finish_level();
            }
        }

        self.wminfo.didsecret = self.players[self.consoleplayer].didsecret;
        self.wminfo.episode = self.game_episode - 1;
        self.wminfo.last = self.game_map;
        dbg!(self.wminfo.episode);

        if !matches!(self.game_mode, GameMode::Commercial) {
            if self.game_map == 8 {
                self.game_action = GameAction::Victory;
                return;
            }
            if self.game_map == 8 {
                for p in self.players.iter_mut() {
                    p.didsecret = true;
                }
            }
        }

        // wminfo.next is 0 biased, unlike gamemap, which is just bloody confusing...
        if matches!(self.game_mode, GameMode::Commercial) {
            if self.level.as_ref().unwrap().secret_exit {
                if self.game_map == 15 {
                    self.wminfo.next = 30;
                } else if self.game_map == 31 {
                    self.wminfo.next = 31;
                }
            } else if self.game_map == 31 || self.game_map == 32 {
                self.wminfo.next = 15;
            } else {
                self.wminfo.next = self.game_map;
            }
        } else if self.level.as_ref().unwrap().secret_exit {
            // go to secret level
            self.wminfo.next = 8;
        } else if self.game_map == 9 {
            match self.game_episode {
                1 => self.wminfo.next = 3,
                2 => self.wminfo.next = 5,
                3 => self.wminfo.next = 6,
                4 => self.wminfo.next = 2,
                _ => {}
            }
        } else {
            self.wminfo.next = self.game_map;
        }

        self.wminfo.maxkills = self.level.as_ref().unwrap().total_level_kills;
        self.wminfo.maxitems = self.level.as_ref().unwrap().total_level_items;
        self.wminfo.maxsecret = self.level.as_ref().unwrap().total_level_secrets;
        self.wminfo.maxfrags = 0;

        // TODO: par times

        for (i, in_game) in self.player_in_game.iter().enumerate() {
            self.wminfo.plyr[i].inn = *in_game;
            self.wminfo.plyr[i].total_kills = self.players[i].total_kills;
            self.wminfo.plyr[i].items_collected = self.players[i].items_collected;
            self.wminfo.plyr[i].secrets_found = self.players[i].secrets_found;
            self.wminfo.plyr[i].level_time = if let Some(level) = &self.level {
                level.level_time
            } else {
                0
            };
            self.wminfo.plyr[i]
                .frags
                .copy_from_slice(&self.players[i].frags);
        }

        self.level = None; // Drop level data
        self.gamestate = GameState::Intermission;
    }

    fn start_finale(&mut self) {
        self.wminfo.didsecret = self.players[self.consoleplayer].didsecret;
        self.wminfo.episode = self.game_episode;
        self.wminfo.last = self.game_map;

        self.gamestate = GameState::Finale;
        self.level = None; // drop the level
        self.game_action = GameAction::None;
    }

    /// The ticker which controls the state the game-exe is in. For example the
    /// game-exe could be in menu mode, demo play, intermission
    /// (`GameState`). A state may also be running other functions that can
    /// change the game-exe state or cause an action through `GameAction`.
    ///
    /// Doom function name `G_Ticker`
    pub fn ticker<I, S, H, F>(&mut self, machinations: &mut Machinations<I, S, H, F>)
    where
        I: MachinationTrait,
        S: MachinationTrait,
        H: MachinationTrait,
        F: MachinationTrait,
    {
        trace!("Entered ticker");
        // do player reborns if needed
        for i in 0..MAXPLAYERS {
            if self.player_in_game[i] && self.players[i].player_state == PlayerState::Reborn {
                self.do_reborn(i);
            }
        }

        if let Some(level) = &mut self.level {
            if let Some(action) = level.game_action.take() {
                self.game_action = action;
                info!("Game state changed: {:?}", self.game_action);
            }
        }

        // do things to change the game-exe state
        match self.game_action {
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
            if self.player_in_game[i] {
                // sets the players cmd for this tic
                self.players[i].cmd = self.netcmds[i][0];
                // memcpy(cmd, &netcmds[i][buf], sizeof(ticcmd_t));
                if self.demo_playback {
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
            if self.player_in_game[i]
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

        self.old_game_state = self.gamestate;

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
                if self.player_in_game[i] && !player.think(level) {
                    // TODO: what to do with dead player?
                }
            }

            unsafe {
                let lev = &mut *(level as *mut Level);
                level.thinkers.run_thinkers(lev);
            }

            level.level_time += 1;

            let animations = &mut self.animations;
            let mut pic_data = self.pic_data.borrow_mut();
            update_specials(level, animations, &mut pic_data);
            respawn_specials(level);
        }
    }
}
