use std::{cell::RefCell, rc::Rc};

use crate::{
    d_main,
    d_main::{DoomOptions, Skill},
    doom_def::*,
    level::Level,
    pic::{PicAnimation, Switches},
    play::{
        map_object::MapObject,
        player::{Player, PlayerState, WBStartStruct},
        specials::{spawn_specials, update_specials},
        utilities::m_clear_random,
    },
    tic_cmd::{TicCmd, TIC_CMD_BUTTONS},
    PicData,
};
use d_main::identify_version;
use log::{debug, error, info, trace, warn};
use wad::WadData;

/// Game is very much driven by d_main, which operates as an orchestrator
pub struct Game {
    /// Contains the full wad file
    pub wad_data: WadData,
    pub level: Option<Level>,
    /// Pre-composed textures, shared to the renderer. `doom-lib` owns and uses
    /// access to change animations + translation tables.
    pub pic_data: Rc<RefCell<PicData>>,
    /// Pre-generated texture animations
    pub animations: Vec<PicAnimation>,
    /// List of switch textures in ordered pairs
    pub switch_list: Vec<usize>,

    running: bool,
    // Game locals
    /// only if started as net death
    deathmatch: bool,
    /// only true if packets are broadcast
    netgame: bool,

    /// Tracks which players are currently active, set by d_net.c loop
    pub player_in_game: [bool; MAXPLAYERS],
    /// Each player in the array may be controlled
    pub players: [Player; MAXPLAYERS],
    /// ?
    turbodetected: [bool; MAXPLAYERS],

    //
    old_game_state: GameState,
    game_action: GameAction,
    game_state: GameState,
    game_skill: Skill,
    respawn_monsters: bool,
    game_episode: i32,
    game_map: i32,
    game_tic: u32,

    /// If non-zero, exit the level after this number of minutes.
    time_limit: Option<i32>,

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
    localcmds: [TicCmd; BACKUPTICS],

    game_mode: GameMode,
    game_mission: GameMission,
    wipe_game_state: GameState,
    usergame: bool,

    /// The options the game exe was started with
    pub options: DoomOptions,
}

impl Game {
    pub fn new(mut options: DoomOptions) -> Game {
        // TODO: a bunch of version checks here to determine what game mode
        let respawn_monsters = matches!(options.skill, d_main::Skill::Nightmare);

        let mut wad = WadData::new(options.iwad.clone().into());

        let (game_mode, game_mission, game_description) = identify_version(&wad);

        debug!("Game: new mode = {:?}", game_mode);
        if game_mode == GameMode::Retail {
            if options.episode > 4 && options.pwad.is_none() {
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

        if let Some(ref pwad) = options.pwad {
            wad.add_file(pwad.into());
        }

        // Mimic the OG output
        println!(
            "\n{} Startup v{}.{}\n",
            game_description,
            DOOM_VERSION / 100,
            DOOM_VERSION % 100
        );

        match game_mode {
            GameMode::Shareware => {
                println!(
                    "==========================================================================="
                );
                println!("                                Shareware!");
                println!(
                    "==========================================================================="
                );
            }
            _ => {
                println!(
                    "==========================================================================="
                );
                println!("                 Commercial product - do not distribute!");
                println!("         Please report software piracy to the SPA: 1-800-388-PIR8");
                println!(
                    "===========================================================================\n"
                );
            }
        }

        let pic_data = PicData::init(&wad);
        println!("Init playloop state.");
        let animations = PicAnimation::init(&pic_data);
        let switch_list = Switches::init(game_mode, &pic_data);
        // TODO: S_Init (sfxVolume * 8, musicVolume * 8);
        // TODO: D_CheckNetGame ();
        // TODO: HU_Init ();
        // TODO: ST_Init ();

        Game {
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
            player_in_game: [true, false, false, false], // TODO: should be set in d_net.c

            paused: false,
            deathmatch: false,
            netgame: false,
            turbodetected: [false; MAXPLAYERS],
            old_game_state: GameState::GS_LEVEL,
            game_action: GameAction::NewGame, // TODO: default to ga_nothing when more state is done
            game_state: GameState::GS_LEVEL,
            game_skill: options.skill,
            game_tic: 0,
            respawn_monsters,
            game_episode: options.episode,
            game_map: options.map,
            time_limit: None,
            consoleplayer: 0,
            displayplayer: 0,
            level_start_tic: 0,
            wminfo: WBStartStruct::default(),

            netcmds: [[TicCmd::new(); BACKUPTICS]; MAXPLAYERS],
            localcmds: [TicCmd::new(); BACKUPTICS],

            game_mode,
            game_mission,
            wipe_game_state: GameState::GS_LEVEL,
            usergame: false,
            options,
        }
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

    pub fn game_mode(&self) -> GameMode {
        self.game_mode
    }

    /// G_InitNew
    /// Can be called by the startup code or the menu task,
    /// consoleplayer, displayplayer, playeringame[] should be set.
    ///
    /// This appears to be defered because the function call can happen at any time
    /// in the game. So rather than just abruptly stop everything we should set
    /// the action so that the right sequences are run. Unsure of impact of
    /// changing game vars beyong action here, probably nothing.
    pub fn defered_init_new(&mut self, skill: Skill, episode: i32, map: i32) {
        self.game_skill = skill;
        self.game_episode = episode;
        self.game_map = map;
        self.game_action = GameAction::NewGame;
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

        // TODO: not pass these, they are stored already
        self.init_new(self.game_skill, self.game_episode, self.game_map);
        self.game_action = GameAction::Nothing;
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
            player.player_state = PlayerState::PstReborn;
        }

        self.paused = false;
        self.game_episode = episode;
        self.game_map = map;
        self.game_skill = skill;
        self.usergame = true; // will be set false if a demo

        self.pic_data
            .borrow_mut()
            .set_sky_pic(self.game_mode, self.game_episode, self.game_map);

        info!("New game!");
        self.do_load_level();
    }

    /// Doom function name `G_DoLoadLevel`
    fn do_load_level(&mut self) {
        debug!("Entered do_load_level");
        // TODO: check and set sky texture, function R_TextureNumForName

        if self.wipe_game_state == GameState::GS_LEVEL {
            self.wipe_game_state = GameState::FORCE_WIPE;
        }
        self.game_state = GameState::GS_LEVEL;

        for player in self.players.iter_mut() {
            if player.player_state == PlayerState::PstDead {
                player.player_state = PlayerState::PstReborn;
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
        self.game_action = GameAction::Nothing;

        let level = unsafe {
            Level::new(
                self.game_skill,
                self.game_episode,
                self.game_map,
                self.game_mode,
                self.switch_list.clone(),
                self.pic_data.clone(),
            )
        };

        info!("Level started: E{} M{}", level.episode, level.game_map);
        self.level = Some(level);

        if let Some(ref mut level) = self.level {
            level.load(&self.wad_data);

            // Pointer stuff must be set up *AFTER* the level data has been allocated
            // (it moves when punted to Some<Level>)
            let thing_list = (*level.map_data.get_things()).to_owned();

            for thing in &thing_list {
                MapObject::p_spawn_map_thing(thing, level, &mut self.players, &self.player_in_game);
            }
            spawn_specials(level);

            debug!("Level: thinkers = {}", &level.thinkers.len());
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
        // TODO: remove after new-game stuff done
        self.pic_data
            .borrow_mut()
            .set_sky_pic(self.game_mode, self.game_episode, self.game_map);

        // TODO: S_Start();
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn set_running(&mut self, run: bool) {
        self.running = run;
    }

    fn do_reborn(&mut self, player_num: usize) {
        info!("Player respawned");
        self.game_action = GameAction::LoadLevel;
        // TODO: deathmatch spawns
    }

    /// Doom function name `G_DoWorldDone`
    fn do_world_done(&mut self) {
        self.game_state = GameState::GS_LEVEL;
        self.game_map = self.wminfo.next + 1;
        self.do_load_level();
        self.game_action = GameAction::Nothing;
        // TODO: viewactive = true;
    }

    fn world_done(&mut self) {
        self.game_action = GameAction::WorldDone;
        if let Some(level) = &self.level {
            if level.secret_exit {
                for p in self.players.iter_mut() {
                    p.didsecret = true;
                }
            }
            if matches!(self.game_mode, GameMode::Commercial) {
                match self.game_map {
                    6 | 11 | 15 | 20 | 30 | 31 => {
                        if !level.secret_exit && (self.game_map == 15 || self.game_map == 31) {
                            // ignore
                        } else {
                            // TODO: F_StartFinale();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn do_completed(&mut self) {
        self.game_action = GameAction::Nothing;

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

        // TODO: temporary
        for (i, in_game) in self.player_in_game.iter().enumerate() {
            if *in_game {
                info!(
                    "Player {i}: Total Items: {}/{}",
                    self.wminfo.plyr[i].sitems, self.wminfo.maxitems
                );
                info!(
                    "Player {i}: Total Kills: {}/{}",
                    self.wminfo.plyr[i].skills, self.wminfo.maxkills
                );
                info!(
                    "Player {i}: Total Secrets: {}/{}",
                    self.wminfo.plyr[i].ssecret, self.wminfo.maxsecret
                );
                info!("Player {i}: Level Time: {}", self.wminfo.plyr[i].stime);
            }
        }

        self.game_state = GameState::GS_INTERMISSION;
    }

    /// G_Ticker
    pub fn ticker(&mut self) {
        trace!("Entered ticker");
        if let Some(level) = &mut self.level {
            if let Some(action) = level.game_action.take() {
                self.game_action = action;
                info!("Game state changed: {:?}", self.game_action);
            }
        }
        // // do player reborns if needed
        for i in 0..MAXPLAYERS {
            if self.player_in_game[i] && self.players[i].player_state == PlayerState::PstReborn {
                self.do_reborn(i);
            }
        }

        // do things to change the game state
        match self.game_action {
            GameAction::LoadLevel => self.do_load_level(),
            GameAction::NewGame => self.do_new_game(),
            GameAction::CompletedLevel => self.do_completed(),
            GameAction::Nothing => {}
            GameAction::LoadGame => todo!("G_DoLoadGame()"),
            GameAction::SaveGame => todo!("G_DoSaveGame()"),
            GameAction::PlayDemo => todo!("G_DoPlayDemo()"),
            GameAction::Victory => todo!("F_StartFinale()"),
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

        if self.old_game_state == GameState::GS_INTERMISSION
            && self.game_state != GameState::GS_INTERMISSION
        {
            //WI_End();
            error!("Do screen wipe");
        }

        self.old_game_state = self.game_state;

        match self.game_state {
            GameState::GS_LEVEL => {
                // P_Ticker(); player movements, run thinkers etc
                self.p_ticker();
                // ST_Ticker();
                // AM_Ticker();
                // HU_Ticker();
            }
            GameState::GS_INTERMISSION => {
                error!("TODO: show end-of-level stats with WI_Ticker()");
                // WI_Ticker calls world_done()
                self.world_done();
            }
            GameState::GS_FINALE => {
                // F_Ticker();
            }
            GameState::GS_DEMOSCREEN => {
                // D_PageTicker();
            }
            GameState::FORCE_WIPE => {
                // do a wipe
            }
        }
    }

    /// P_Ticker
    pub fn p_ticker(&mut self) {
        if self.paused {
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
        }

        update_specials(self);
    }
}
