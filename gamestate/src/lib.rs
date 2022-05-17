//! Game state, fairly self-descriptive but bares expanding on in a little more detail.
//!
//! The state of the game can be a few states only:
//!
//! - level playing
//! - intermission/finale
//! - demo playing
//! - screen wipe
//!
//! The game state can be changed by a few actions - these are more concretely
//! defined as trait functions in `GameTraits`, where the exposed functions trigger
//! an action from `GameAction`. When an action is set it takes effect on the next tic.
//!
//! Note that the primary state is either demo-play or level-play.
//!
//! The active game state also determines which `Machinations` are run, and the order
//! in which they run - these are such things as intermission screens or statusbars
//! during gameplay. In the case of a statusbar for example it ticks only during the
//! `GameState::Level` state, and draws to the buffer after the player view is drawn.
//!

pub mod game_impl;
pub mod machination;

use std::{cell::RefCell, rc::Rc, thread::JoinHandle, time::Duration};

use crate::machination::Machinations;
use gameplay::{
    log,
    log::{debug, error, info, trace, warn},
    m_clear_random, spawn_specials,
    tic_cmd::{TicCmd, TIC_CMD_BUTTONS},
    update_specials, GameAction, GameMission, GameMode, Level, MapObjFlag, MapObject, PicAnimation,
    PicData, Player, PlayerState, Skill, Switches, WBStartStruct, MAXPLAYERS,
};
use gamestate_traits::{GameState, GameTraits, MachinationTrait};
use sdl2::AudioSubsystem;
use sound_sdl2::SndServerTx;
use sound_traits::{MusTrack, SoundAction, SoundServer, SoundServerTic};
use wad::{lumps::WadPatch, WadData};

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
#[derive(Debug)]
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
    pub episode: i32,
    pub map: i32,
    pub autostart: bool,
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
            verbose: log::LevelFilter::Info,
        }
    }
}

fn identify_version(wad: &wad::WadData) -> (GameMode, GameMission, &'static str) {
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
    pub title: WadPatch,
    /// Contains the full wad file. Wads are tiny in terms of today's memory use
    /// so it doesn't hurt to store the full file in ram. May change later.
    pub wad_data: WadData,
    /// The complete `Level` data encompassing the everything everywhere all at once...
    /// (if loaded).
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
    game_episode: i32,
    game_map: i32,
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
    _snd_thread: JoinHandle<()>,
}

impl Drop for Game {
    fn drop(&mut self) {
        self.snd_command.send(SoundAction::Shutdown).unwrap();
        // Nightly only
        // while !self.snd_thread.is_finished() {}
        std::thread::sleep(Duration::from_millis(100));
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

        let pic_data = PicData::init(&wad);
        info!("Init playloop state.");
        let animations = PicAnimation::init(&pic_data);
        let switch_list = Switches::init(game_mode, &pic_data);

        let mut snd_server = sound_sdl2::Snd::new(snd_ctx, &wad).unwrap();
        let tx = snd_server.init().unwrap();
        let snd_thread = std::thread::spawn(move || loop {
            if !snd_server.tic() {
                break;
            }
        });
        tx.send(SoundAction::SfxVolume(sfx_vol)).unwrap();
        tx.send(SoundAction::MusicVolume(mus_vol)).unwrap();
        // TODO: D_CheckNetGame ();
        // TODO: HU_Init ();
        // TODO: ST_Init ();

        let mut game_action = GameAction::None;
        let gamestate = GameState::Demo;
        if options.warp {
            game_action = GameAction::NewGame;
        }

        let lump = wad.get_lump("TITLEPIC").expect("TITLEPIC missing");
        let title = WadPatch::from_lump(lump);

        Game {
            title,
            wad_data: wad,
            level: None,
            running: true,
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
            wipe_game_state: GameState::Demo,
            usergame: false,
            options,
            snd_command: tx,
            _snd_thread: snd_thread,
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
        self.respawn_monsters = false;
        self.consoleplayer = 0;
        self.player_in_game[0] = true;

        // TODO: not pass these, they are stored already
        self.init_new(self.game_skill, self.game_episode, self.game_map);
        self.game_action = GameAction::None;
    }

    fn init_new(&mut self, skill: Skill, mut episode: i32, mut map: i32) {
        debug!("Entered init_new");

        if self.paused {
            self.paused = false;
            // TODO: S_ResumeSound();
        }

        debug!("Game: init_new: mode = {:?}", self.game_mode);
        if self.game_mode == GameMode::Retail {
            if episode > 4 {
                warn!(
                    "Game: init_new: {:?} mode but episode {} is greater than 4",
                    self.game_mode, episode
                );
                episode = 4;
            }
        } else if self.game_mode == GameMode::Shareware {
            if episode > 1 {
                warn!(
                    "Game: init_new: {:?} mode but episode {} is greater than 1",
                    self.game_mode, episode
                );
                episode = 1; // only start episode 1 on shareware
            }
            if map > 5 {
                warn!(
                    "Game: init_new: {:?} mode but map {} is greater than 5",
                    self.game_mode, map
                );
                map = 5;
            }
        } else if episode > 3 {
            warn!(
                "Game: init_new: {:?} mode but episode {} is greater than 3",
                self.game_mode, episode
            );
            episode = 3;
        }

        if map > 9 && self.game_mode != GameMode::Commercial {
            warn!(
                "Game: init_new: {:?} mode but map {} is greater than 9",
                self.game_mode, map
            );
            map = 9;
        }

        m_clear_random();

        if skill == Skill::Nightmare || self.options.respawn_parm {
            self.respawn_monsters = true;
        } else {
            self.respawn_monsters = false;
        }

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
        self.game_episode = episode;
        self.game_map = map;
        self.game_skill = skill;
        self.usergame = true; // will be set false if a demo

        self.pic_data
            .borrow_mut()
            .set_sky_pic(self.game_mode, self.game_episode, self.game_map);

        info!("New game-exe!");
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
            player.killcount = 0;
            player.secretcount = 0;
            player.itemcount = 0;
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
            level.load(&self.pic_data.borrow_mut(), &self.wad_data);

            // Pointer stuff must be set up *AFTER* the level data has been allocated
            // (it moves when punted to Some<Level>)
            let thing_list = (*level.map_data.things()).to_owned();

            for thing in &thing_list {
                MapObject::p_spawn_map_thing(
                    thing,
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

    fn do_reborn(&mut self, player_num: usize) {
        info!("Player respawned");
        self.game_action = GameAction::LoadLevel;
        // self.players[player_num].
        // TODO: deathmatch spawns
    }

    /// Load the next level and set the `GameAction` to None
    ///
    /// Doom function name `G_DoWorldDone`
    fn do_world_done(&mut self) {
        self.gamestate = GameState::Level;
        self.game_map = self.wminfo.next + 1;
        self.do_load_level();
        self.game_action = GameAction::None;
        // TODO: viewactive = true;
    }

    /// Cleanup, re-init, and set up for next level or episode. Also sets up info
    /// that can be displayed on the intermission screene.
    fn do_completed(&mut self) {
        self.game_action = GameAction::None;

        for (i, in_game) in self.player_in_game.iter().enumerate() {
            if *in_game {
                let player = &mut self.players[i];
                player.finish_level();
            }
        }

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

        self.wminfo.didsecret = self.players[self.consoleplayer].didsecret;
        self.wminfo.epsd = self.game_episode - 1;
        self.wminfo.last = self.game_map - 1;

        // wminfo.next is 0 biased, unlike gamemap, which is just bloody confusing...
        if matches!(self.game_mode, GameMode::Commercial) {
            if self.level.as_ref().unwrap().secret_exit {
                if self.game_map == 15 {
                    self.wminfo.next = 30;
                } else if self.game_map == 31 {
                    self.wminfo.next = 31;
                }
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

        self.wminfo.maxkills = self.level.as_ref().unwrap().totalkills;
        self.wminfo.maxitems = self.level.as_ref().unwrap().totalitems;
        self.wminfo.maxsecret = self.level.as_ref().unwrap().totalsecret;
        self.wminfo.maxfrags = 0;

        // TODO: par times

        for (i, in_game) in self.player_in_game.iter().enumerate() {
            self.wminfo.plyr[i].inn = *in_game;
            self.wminfo.plyr[i].skills = self.players[i].killcount;
            self.wminfo.plyr[i].sitems = self.players[i].itemcount;
            self.wminfo.plyr[i].ssecret = self.players[i].secretcount;
            self.wminfo.plyr[i].stime = if let Some(level) = &self.level {
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

    /// The ticker which controls the state the game-exe is in. For example the game-exe could be
    /// in menu mode, demo play, intermission (`GameState`). A state may also be
    /// running other functions that can change the game-exe state or cause an action
    /// through `GameAction`.
    ///
    /// Doom function name `G_Ticker`
    pub fn ticker<I, S>(&mut self, machinations: &mut Machinations<I, S>)
    where
        I: MachinationTrait,
        S: MachinationTrait,
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
            GameAction::LoadLevel => self.do_load_level(),
            GameAction::NewGame => self.do_new_game(),
            GameAction::CompletedLevel => {
                self.do_completed();
                machinations.intermission.init(self);
            }
            GameAction::None => {}
            GameAction::LoadGame => todo!("G_DoLoadGame()"),
            GameAction::SaveGame => todo!("G_DoSaveGame()"),
            GameAction::PlayDemo => todo!("G_DoPlayDemo()"),
            GameAction::Victory => {
                // TODO: temporary to allow Doom 2 to continue
                if self.game_mode == GameMode::Commercial && self.game_map == 7 {
                    error!("DOOM II finale for Map07 not done. Using GameAction::CompletedLevel");
                    self.game_action = GameAction::CompletedLevel
                } else {
                    todo!("F_StartFinale()")
                }
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
                let cmd = &self.players[i].cmd;

                // if (demoplayback)
                //     G_ReadDemoTiccmd(cmd);
                // if (demorecording)
                //     G_WriteDemoTiccmd(cmd);

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
                    //         (players[i].cmd.buttons & BTS_SAVEMASK) >> BTS_SAVESHIFT;
                    //     gameaction = ga_savegame;
                    //     break;
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
                // HU_Ticker();
                self.hu_ticker();
            }
            GameState::Intermission => {
                // WI_Ticker calls world_done()
                machinations.intermission.ticker(self);
            }
            GameState::Finale => {
                // F_Ticker();
            }
            GameState::Demo => {
                // D_PageTicker();
            }
            GameState::ForceWipe => {
                // do a wipe
            }
        }
    }

    /// Gameplay ticker. Updates the game-exe level state along with all thinkers inside
    /// that level. Also watches for `TicCmd` that initiate another action or state such
    /// as pausing in menus, demo recording, save/load.
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

            // P_RespawnSpecials ();

            level.level_time += 1;

            let animations = &mut self.animations;
            let mut pic_data = self.pic_data.borrow_mut();
            update_specials(level, animations, &mut pic_data);
        }
    }

    /// TODO: temporary to get player messages in CLI out
    fn hu_ticker(&mut self) {
        if let Some(ref mut level) = self.level {
            for (i, player) in self.players.iter_mut().enumerate() {
                if self.player_in_game[i] {
                    if let Some(msg) = player.message.take() {
                        info!("Console: {msg}");
                    }
                }
            }
        }
    }
}
