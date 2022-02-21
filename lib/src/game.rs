use crate::d_main;
use crate::d_main::{GameOptions, Skill};
use crate::level_data::{p_ticker, Level};
use crate::play::{
    map_object::MapObject,
    player::{Player, PlayerState, WBStartStruct},
    specials::spawn_specials,
    utilities::m_clear_random,
};
use crate::renderer::bsp::BspRenderer;
use crate::renderer::plane::VisPlaneCtrl;
use crate::renderer::RenderData;
use crate::tic_cmd::TicCmd;
use crate::{doom_def::*, tic_cmd::TIC_CMD_BUTTONS};
use d_main::identify_version;
use log::{debug, info, trace, warn};
use sdl2::{rect::Rect, render::Canvas, surface::Surface};
use std::io::Write;
use wad::WadData;

/// Game is very much driven by d_main, which operates as an orchestrator
pub struct Game {
    /// Contains the full wad file
    wad_data: WadData,
    pub level: Option<Level>,

    bsp_renderer: BspRenderer,
    r_data: RenderData,
    visplanes: VisPlaneCtrl,

    pub crop_rect: Rect,

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
    game_episode: u32,
    game_map: u32,
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
    /// for intermission
    totalkills: i32,
    /// for intermission
    totalitems: i32,
    /// for intermission
    totalsecret: i32,
    secret_exit: bool,

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
    pub game_options: GameOptions,
}

impl Game {
    pub fn new(mut options: GameOptions) -> Game {
        let mut logger = env_logger::Builder::new();
        logger
            .target(env_logger::Target::Stdout)
            .format(|buf, record| writeln!(buf, "{}: {}", record.level(), record.args()))
            .filter(None, options.verbose)
            .init();

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

        Game {
            wad_data: wad,
            level: None,
            crop_rect: Rect::new(0, 0, 1, 1),

            r_data: RenderData::default(),
            visplanes: VisPlaneCtrl::default(),
            bsp_renderer: BspRenderer::default(),

            running: true,

            players: [
                Player::default(),
                Player::default(),
                Player::default(),
                Player::default(),
            ],
            player_in_game: [true, false, false, false], // should be set in d_net.c

            paused: false,
            deathmatch: false,
            netgame: false,
            turbodetected: [false; MAXPLAYERS],
            old_game_state: GameState::GS_LEVEL,
            game_action: GameAction::ga_loadlevel, // TODO: default to ga_nothing when more state is done
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
            totalkills: 0,
            totalitems: 0,
            totalsecret: 0,
            secret_exit: false,
            wminfo: WBStartStruct::default(),

            netcmds: [[TicCmd::new(); BACKUPTICS]; MAXPLAYERS],
            localcmds: [TicCmd::new(); BACKUPTICS],

            game_mode,
            game_mission,
            wipe_game_state: GameState::GS_LEVEL,
            usergame: false,
            game_options: options,
        }
    }

    /// G_InitNew
    /// Can be called by the startup code or the menu task,
    /// consoleplayer, displayplayer, playeringame[] should be set.
    ///
    /// This appears to be defered because the function call can happen at any time
    /// in the game. So rather than just abruptly stop everything we should set
    /// the action so that the right sequences are run. Unsure of impact of
    /// changing game vars beyong action here, probably nothing.
    pub fn defered_init_new(&mut self, skill: Skill, episode: u32, map: u32) {
        self.game_skill = skill;
        self.game_episode = episode;
        self.game_map = map;
        self.game_action = GameAction::ga_newgame;
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
        self.game_action = GameAction::ga_nothing;
    }

    fn init_new(&mut self, skill: Skill, mut episode: u32, mut map: u32) {
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

        if skill == Skill::Nightmare || self.game_options.respawn_parm {
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

        // TODO: set the sky level for the episode
        // if (gamemode == commercial)
        // {
        //     skytexture = R_TextureNumForName("SKY3");
        //     if (gamemap < 12)
        //         skytexture = R_TextureNumForName("SKY1");
        //     else if (gamemap < 21)
        //         skytexture = R_TextureNumForName("SKY2");
        // }
        // else
        //     switch (episode)
        //     {
        //     case 1:
        //         skytexture = R_TextureNumForName("SKY1");
        //         break;
        //     case 2:
        //         skytexture = R_TextureNumForName("SKY2");
        //         break;
        //     case 3:
        //         skytexture = R_TextureNumForName("SKY3");
        //         break;
        //     case 4: // Special Edition sky
        //         skytexture = R_TextureNumForName("SKY4");
        //         break;
        //     }
        println!("New game!");
    }

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
        self.game_action = GameAction::ga_nothing;

        let level = unsafe {
            Level::new(
                self.game_skill,
                self.game_episode,
                self.game_map,
                self.game_mode,
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
        self.totalkills = 0;
        self.totalitems = 0;
        self.totalsecret = 0;
        self.wminfo.maxfrags = 0;
        self.wminfo.partime = 180;
        self.players[self.consoleplayer].viewz = 1.0;

        // TODO: S_Start();
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn set_running(&mut self, run: bool) {
        self.running = run;
    }

    fn do_reborn(&mut self, player_num: usize) {
        self.game_action = GameAction::ga_loadlevel;
        // TODO: deathmatch spawns
    }

    /// G_Ticker
    pub fn ticker(&mut self) {
        trace!("Entered ticker");
        if let Some(ref level) = self.level {
            if let Some(action) = level.game_action {
                self.game_action = action;
                self.secret_exit = level.secret_exit;
            }
        }
        // // do player reborns if needed
        // for (i = 0; i < MAXPLAYERS; i++)
        // if (playeringame[i] && players[i].playerstate == PST_REBORN)
        //     G_DoReborn(i);
        for i in 0..MAXPLAYERS {
            if self.player_in_game[i] && self.players[i].player_state == PlayerState::PstReborn {
                self.do_reborn(i);
            }
        }

        // // do things to change the game state
        // while (gameaction != ga_nothing)
        // {
        //     switch (gameaction)
        //     {
        //     case ga_loadgame:
        //         G_DoLoadGame();
        //         break;
        //     case ga_savegame:
        //         G_DoSaveGame();
        //         break;
        //     case ga_playdemo:
        //         G_DoPlayDemo();
        //         break;
        //     case ga_completed:
        //         G_DoCompleted();
        //         break;
        //     case ga_victory:
        //         F_StartFinale();
        //         break;
        //     case ga_worlddone:
        //         G_DoWorldDone();
        //         break;
        //     case ga_screenshot:
        //         M_ScreenShot();
        //         gameaction = ga_nothing;
        //         break;
        //     case ga_nothing:
        //         break;
        //     }
        // }
        match self.game_action {
            GameAction::ga_loadlevel => self.do_load_level(),
            GameAction::ga_newgame => self.do_new_game(),
            _ => {}
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

        match self.game_state {
            GameState::GS_LEVEL => {
                p_ticker(self); // P_Ticker(); player movements, run thinkers etc
                                // ST_Ticker();
                                // AM_Ticker();
                                // HU_Ticker();
            }
            GameState::GS_INTERMISSION => {
                //WI_Ticker();
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

    /// D_Display
    // TODO: Move
    pub fn render_player_view(&mut self, canvas: &mut Canvas<Surface>) {
        if !self.player_in_game[0] {
            return;
        }

        if let Some(ref mut level) = self.level {
            let map = &level.map_data;

            let player = &mut self.players[self.consoleplayer];

            self.visplanes.clear_planes();
            self.bsp_renderer.clear_clip_segs();
            self.r_data.clear_data();
            // The state machine will handle which state renders to the surface
            //self.states.render(dt, &mut self.canvas);

            let colour = sdl2::pixels::Color::RGBA(90, 80, 80, 255);
            canvas.set_draw_color(colour);
            canvas.fill_rect(Rect::new(0, 0, 320, 100)).unwrap();
            let colour = sdl2::pixels::Color::RGBA(90, 90, 90, 255);
            canvas.set_draw_color(colour);
            canvas.fill_rect(Rect::new(0, 100, 320, 100)).unwrap();
            self.bsp_renderer.render_bsp_node(
                map,
                player,
                map.start_node(),
                &mut self.r_data,
                canvas,
            );
        }
    }
}
