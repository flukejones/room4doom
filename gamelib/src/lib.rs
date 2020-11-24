use std::{f32::consts::{FRAC_PI_2, FRAC_PI_4}};

use crate::p_map_object::MapObject;
use crate::r_bsp::Bsp;
use angle::Angle;
use d_main::{GameOptions, Skill};
use d_thinker::{Think, Thinker};
use doom_def::*;
use player::{Player, WBStartStruct};
use sdl2::surface::Surface;
use sdl2::{render::Canvas, video::Window};
use wad::Wad;

pub mod angle;
pub mod d_main;
pub mod d_thinker;
pub mod doom_def;
pub mod entities;
pub mod flags;
pub mod info;
pub mod input;
pub mod p_enemy;
pub mod p_local;
pub mod p_map;
pub mod p_map_object;
pub mod p_player_sprite;
pub mod p_spec;
pub mod player;
pub mod r_bsp;
pub mod r_segs;
pub mod sounds;
pub mod timestep;
pub mod p_lights;

/// Game is very much driven by d_main, which operates as an orchestrator
pub struct Game {
    _wad:    Wad,
    map:     Option<Bsp>,
    running: bool,

    think_mobj: Vec<Thinker<MapObject>>,

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
        let mut map = Bsp::new("E1M1".to_owned());
        map.load(&wad);

        let mut players = [
            Player::default(),
            Player::default(),
            Player::default(),
            Player::default(),
        ];

        let player_thing = &map.get_things()[0];
        MapObject::p_spawn_player(player_thing, &map, &mut players);

        Game {
            _wad: wad,
            map: Some(map),
            running: true,

            players,
            player_in_game: [false; 4],
            think_mobj: Vec::with_capacity(200),

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
        }
    }

    pub fn running(&self) -> bool { self.running }

    pub fn set_running(&mut self, run: bool) { self.running = run; }

    // D_RunFrame is the main loop, calls many functions:
    //  - Screen wipe maybe
    //    + I_UpdateNoBlit // update framebuffer
    //    + M_Drawer       // menu
    //    + I_FinishUpdate // page flip or blit buffer
    //  - I_StartFrame     // frame sync?
    //  - TryRunTics
    //    + Runs callback to RunTic
    //      - G_Ticker
    //        + P_Ticker
    //          - P_RunThinkers
    //        + WI_Ticker  // screen wipe
    //  - S_UpdateSounds
    //  - D_Display then:
    //    + screen wipe or
    //    + I_FinishUpdate // page flip or blit buffer
    // D_DoomLoop
    //
    // D_Display
    //

    pub fn do_tic(&mut self) {
        //
    }

    /// D_Display
    // TODO: Move one level up to d_main
    pub fn d_display(&mut self, canvas: &mut Canvas<Surface>) {
        let map = self.map.as_mut().unwrap();
        let player = &mut self.players[self.displayplayer];
        map.clear_clip_segs();

        // The state machine will handle which state renders to the surface
        //self.states.render(dt, &mut self.canvas);
        let player_subsect = map.point_in_subsector(&player.xy).unwrap();
        player.viewz = player_subsect.sector.floor_height as f32 + 41.0;
        player.sub_sector = Some(player_subsect); //DPtr::new(player_subsect);

        canvas.clear();
        map.draw_bsp(player, map.start_node(), canvas);
    }

    // TODO: Move one level up to d_main
    pub fn i_finish_update(
        &self,
        canvas: Canvas<Surface>,
        window: &mut Canvas<Window>,
    ) {
        //canvas.present();

        let texture_creator = window.texture_creator();
        let t = canvas.into_surface().as_texture(&texture_creator).unwrap();

        window.copy(&t, None, None).unwrap();
        //self.draw_automap();
        window.present();
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
