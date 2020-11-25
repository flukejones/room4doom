#![feature(const_fn_floating_point_arithmetic)]
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use crate::p_map_object::MapObject;
use crate::r_bsp::BspCtrl;
use angle::Angle;
use d_main::{GameOptions, Skill};
use d_thinker::Thinker;
use doom_def::*;
use map_data::MapData;
use p_map::MobjCtrl;
use player::{Player, WBStartStruct};
use sdl2::render::Canvas;
use sdl2::surface::Surface;
use tic_cmd::{TicCmd, TIC_CMD_BUTTONS};
use wad::Wad;

pub mod angle;
pub mod d_main;
pub mod d_thinker;
pub mod doom_def;
pub mod entities;
pub mod flags;
pub mod info;
pub mod input;
pub mod map_data;
pub mod p_enemy;
pub mod p_lights;
pub mod p_local;
pub mod p_map;
pub mod p_map_object;
pub mod p_player_sprite;
pub mod p_spec;
pub mod player;
pub mod r_bsp;
pub mod r_segs;
pub mod sounds;
pub mod tic_cmd;
pub mod timestep;

pub struct Level {
    pub map_data:  MapData,
    pub bsp_ctrl:  BspCtrl,
    pub mobj_ctrl: MobjCtrl,
    pub thinkers:  Vec<Thinker<MapObject>>,
}
impl Level {
    fn new(map_data: MapData) -> Self {
        Level {
            map_data,
            bsp_ctrl: BspCtrl::default(),
            mobj_ctrl: MobjCtrl::default(),
            thinkers: Vec::with_capacity(200),
        }
    }
}

/// Game is very much driven by d_main, which operates as an orchestrator
pub struct Game {
    /// Contains the full wad file
    wad_data: Wad,
    level:    Option<Level>,

    running:    bool,
    // Game locals
    /// only if started as net death
    deathmatch: i32,
    /// only true if packets are broadcast
    netgame:    bool,

    /// Tracks which players are currently active, set by d_net.c loop
    player_in_game: [bool; MAXPLAYERS],
    /// Each player in the array may be controlled
    pub players:    [Player; MAXPLAYERS],
    /// ?
    turbodetected:  [bool; MAXPLAYERS],

    //
    old_game_state:   GameState,
    game_action:      GameAction,
    game_state:       GameState,
    game_skill:       Skill,
    respawn_monsters: bool,
    game_episode:     u32,
    game_map:         u32,

    /// If non-zero, exit the level after this number of minutes.
    time_limit: Option<i32>,

    /// player taking events and displaying
    consoleplayer: usize,
    /// view being displayed        
    displayplayer: usize,
    /// gametic at level start              
    levelstarttic: i32,
    /// for intermission
    totalkills:    i32,
    /// for intermission
    totalitems:    i32,
    /// for intermission
    totalsecret:   i32,

    wminfo: WBStartStruct,

    /// d_net.c
    netcmds:   [[TicCmd; BACKUPTICS]; MAXPLAYERS],
    /// d_net.c
    localcmds: [TicCmd; BACKUPTICS],
}

impl Game {
    pub fn new(options: GameOptions) -> Game {
        // TODO: a bunch of version checks here to determine what game mode
        let respawn_monsters = match options.start_skill {
            d_main::Skill::Nightmare => true,
            _ => false,
        };

        let mut wad = Wad::new(options.iwad);
        wad.read_directories();

        Game {
            wad_data: wad,
            level: None,

            running: true,

            players: [
                Player::default(),
                Player::default(),
                Player::default(),
                Player::default(),
            ],
            player_in_game: [false; 4],

            deathmatch: 0,
            netgame: false,
            turbodetected: [false; MAXPLAYERS],
            old_game_state: GameState::GS_LEVEL,
            game_action: GameAction::ga_nothing,
            game_state: GameState::GS_LEVEL,
            game_skill: options.start_skill,
            respawn_monsters,
            game_episode: options.start_episode,
            game_map: options.start_map,
            time_limit: None,
            consoleplayer: 0,
            displayplayer: 0,
            levelstarttic: 0,
            totalkills: 0,
            totalitems: 0,
            totalsecret: 0,
            wminfo: WBStartStruct::default(),

            netcmds: [[TicCmd::new(); BACKUPTICS]; MAXPLAYERS],
            localcmds: [TicCmd::new(); BACKUPTICS],
        }
    }

    pub fn load(&mut self) {
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&self.wad_data);

        let player_thing = map.get_things()[0].clone();
        self.level = Some(Level::new(map));

        MapObject::p_spawn_player(
            &player_thing,
            &self.level.as_ref().unwrap().map_data,
            &mut self.players,
        );
        self.player_in_game[0] = true;
    }

    pub fn running(&self) -> bool { self.running }

    pub fn set_running(&mut self, run: bool) { self.running = run; }

    /// G_Ticker
    pub fn ticker(&mut self) {
        // // do player reborns if needed
        // for (i = 0; i < MAXPLAYERS; i++)
        // if (playeringame[i] && players[i].playerstate == PST_REBORN)
        //     G_DoReborn(i);

        // // do things to change the game state
        // while (gameaction != ga_nothing)
        // {
        //     switch (gameaction)
        //     {
        //     case ga_loadlevel:
        //         G_DoLoadLevel();
        //         break;
        //     case ga_newgame:
        //         G_DoNewGame();
        //         break;
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

        // get commands, check consistancy,
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
            if self.player_in_game[i] {
                if self.players[i].cmd.buttons & TIC_CMD_BUTTONS.bt_special > 0
                {
                    let mask = self.players[i].cmd.buttons
                        & TIC_CMD_BUTTONS.bt_specialmask;
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
        }

        match self.game_state {
            GameState::GS_LEVEL => {
                // P_Ticker(); // player movements, run thinkers etc
                d_thinker::ticker(self);
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
        }
    }

    /// D_Display
    // TODO: Move
    pub fn render_player_view(&mut self, canvas: &mut Canvas<Surface>) {
        if let Some(ref mut level) = self.level {
            let map = &level.map_data;

            let player = &mut self.players[self.consoleplayer];

            level.bsp_ctrl.clear_clip_segs();
            // The state machine will handle which state renders to the surface
            //self.states.render(dt, &mut self.canvas);
            let player_subsect = map.point_in_subsector(&player.xy).unwrap();
            player.viewz = player_subsect.sector.floor_height as f32 + 41.0;
            player.sub_sector = Some(player_subsect); //DPtr::new(player_subsect);

            canvas.clear();
            level.bsp_ctrl
                .draw_bsp(&map, player, map.start_node(), canvas);
        }
    }
}

/// R_PointToDist
fn point_to_dist(x: f32, y: f32, object: &Player) -> f32 {
    let mut dx = (x - object.xy.x()).abs();
    let mut dy = (y - object.xy.y()).abs();

    if dy > dx {
        let temp = dx;
        dx = dy;
        dy = temp;
    }

    let dist = (dx.powi(2) + dy.powi(2)).sqrt();
    dist
}

/// R_ScaleFromGlobalAngle
// All should be in rads
fn scale(
    visangle: Angle,
    rw_normalangle: Angle,
    rw_distance: f32,
    view_angle: Angle,
) -> f32 {
    static MAX_SCALEFACTOR: f32 = 64.0;
    static MIN_SCALEFACTOR: f32 = 0.00390625;

    let anglea = Angle::new(FRAC_PI_2 + visangle.rad() - view_angle.rad()); // CORRECT
    let angleb = Angle::new(FRAC_PI_2 + visangle.rad() - rw_normalangle.rad()); // CORRECT

    let sinea = anglea.sin(); // not correct?
    let sineb = angleb.sin();

    //            projection
    //m_iDistancePlayerToScreen = m_HalfScreenWidth / HalfFOV.GetTanValue();
    let p = 160.0 / (FRAC_PI_4).tan();
    let num = p * sineb; // oof a bit
    let den = rw_distance * sinea;

    let mut scale = num / den;

    if scale > MAX_SCALEFACTOR {
        scale = MAX_SCALEFACTOR;
    } else if MIN_SCALEFACTOR > scale {
        scale = MIN_SCALEFACTOR;
    }
    scale
}
