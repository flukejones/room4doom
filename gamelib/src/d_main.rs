use std::{error::Error, fmt, str::FromStr};

use gumdrop::Options;
use sdl2::{keyboard::Scancode, pixels::PixelFormatEnum, render::Canvas, rect::Rect, surface::Surface, video::Window};

use crate::{
    doom_def::GameMission, doom_def::GameMode, game::Game, input::Input,
    timestep::TimeStep,
};

#[derive(Debug)]
pub enum DoomArgError {
    InvalidSkill(String),
}

impl Error for DoomArgError {}

impl fmt::Display for DoomArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DoomArgError::InvalidSkill(m) => write!(f, "{}", m),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Skill {
    NoItems = -1, // the "-skill 0" hack
    Baby    = 0,
    Easy,
    Medium,
    Hard,
    Nightmare,
}

impl Default for Skill {
    fn default() -> Self { Skill::Medium }
}

impl FromStr for Skill {
    type Err = DoomArgError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(Skill::Baby),
            "1" => Ok(Skill::Easy),
            "2" => Ok(Skill::Medium),
            "3" => Ok(Skill::Hard),
            "4" => Ok(Skill::Nightmare),
            _ => Err(DoomArgError::InvalidSkill("Invalid arg".to_owned())),
        }
    }
}

#[derive(Debug, Options)]
pub struct GameOptions {
    #[options(no_short, help = "path to game WAD", default = "./doom1.wad")]
    pub iwad:       String,
    #[options(no_short, help = "path to patch WAD")]
    pub pwad:       Option<String>,
    #[options(help = "resolution width in pixels", default = "640")]
    pub width:      u32,
    #[options(help = "resolution height in pixels", default = "480")]
    pub height:     u32,
    #[options(help = "fullscreen?")]
    pub fullscreen: bool,

    #[options(help = "Disable monsters")]
    pub no_monsters:  bool,
    #[options(help = "Monsters respawn after being killed")]
    pub respawn_parm: bool,
    #[options(help = "Monsters move faster")]
    pub fast_parm:    bool,
    #[options(
        no_short,
        help = "Developer mode. F1 saves a screenshot in the current working directory"
    )]
    pub dev_parm:     bool,
    #[options(
        help = "Start a deathmatch game: 1 = classic, 2 = Start a deathmatch 2.0 game.  Weapons do not stay in place and all items respawn after 30 seconds"
    )]
    pub deathmatch:   u8,
    #[options(
        help = "Set the game skill, 1-5 (1: easiest, 5: hardest). A skill of 0 disables all monsters"
    )]
    pub skill:        Skill,
    #[options(help = "Select episode", default = "1")]
    pub episode:      u32,
    #[options(help = "Select map in episode", default = "1")]
    pub map:          u32,
    pub autostart:    bool,
    #[options(help = "game options help")]
    pub help:         bool,
}

pub fn identify_version(wad: &wad::Wad) -> (GameMode, GameMission, String) {
    let game_mode;
    let game_mission;
    let game_description;

    if wad.find_lump_index("MAP01").is_some() {
        game_mission = GameMission::Doom2;
    } else if wad.find_lump_index("E1M1").is_some() {
        game_mission = GameMission::Doom;
    } else {
        panic!("Could not determine IWAD type");
    }

    if game_mission == GameMission::Doom {
        // Doom 1.  But which version?
        if wad.find_lump_index("E4M1").is_some() {
            game_mode = GameMode::Retail;
            game_description = String::from("The Ultimate DOOM");
        } else if wad.find_lump_index("E3M1").is_some() {
            game_mode = GameMode::Registered;
            game_description = String::from("DOOM Registered");
        } else {
            game_mode = GameMode::Shareware;
            game_description = String::from("DOOM Shareware");
        }
    } else {
        game_mode = GameMode::Commercial;
        game_description = String::from("DOOM 2: Hell on Earth");
        // TODO: check for TNT or Plutonia
    }
    (game_mode, game_mission, game_description)
}

pub fn d_doom_loop(
    mut game: Game,
    mut input: Input,
    mut canvas: Canvas<Window>,
) {
    let wsize = canvas.output_size().unwrap();
    let ratio = wsize.1 / 3;
    let xw = ratio * 4;
    let xp = (wsize.0 - xw) / 2;
    game.crop_rect = Rect::new(xp as i32, 0,xw,wsize.1);

    let mut timestep = TimeStep::new();

    'running: loop {
        if !game.running() {
            break 'running;
        }

        try_run_tics(&mut game, &mut input, &mut timestep);
        // TODO: S_UpdateSounds(players[consoleplayer].mo); // move positional sounds
        let surface = Surface::new(320, 200, PixelFormatEnum::RGB555).unwrap();
        let drawer = surface.into_canvas().unwrap();
        // inputs are outside of tic loop?
        d_display(&mut game, drawer, &mut canvas);
    }
}

/// D_Display
/// Does a bunch of stuff in Doom...
pub fn d_display(
    game: &mut Game,
    mut canvas: Canvas<Surface>,
    window: &mut Canvas<Window>,
) {
    //if (gamestate == GS_LEVEL && !automapactive && gametic)
    game.render_player_view(&mut canvas);

    // // menus go directly to the screen
    // TODO: M_Drawer();	 // menu is drawn even on top of everything
    // net update does i/o and buildcmds...
    // TODO: NetUpdate(); // send out any new accumulation

    // consume the canvas
    i_finish_update(canvas, window, game.crop_rect);
}

/// Page-flip or blit to screen
pub fn i_finish_update(canvas: Canvas<Surface>, window: &mut Canvas<Window>, crop_rect: Rect) {
    //canvas.present();

    let texture_creator = window.texture_creator();
    let t = canvas.into_surface().as_texture(&texture_creator).unwrap();

    window.copy(&t, None, Some(crop_rect)).unwrap();
    window.present();
}

fn try_run_tics(game: &mut Game, input: &mut Input, timestep: &mut TimeStep) {
    // TODO: net.c starts here
    input.update(); // D_ProcessEvents

    let console_player = game.consoleplayer;
    // net update does i/o and buildcmds...
    // TODO: NetUpdate(); // send out any new accumulation

    // temporary block
    game.set_running(!input.get_quit());

    // TODO: Network code would update each player slot with incoming TicCmds...
    let cmd = input.tic_events.build_tic_cmd(&input.config);
    game.netcmds[console_player][0] = cmd;

    // Special key check
    if input.tic_events.is_kb_pressed(Scancode::Escape) {
        game.set_running(false);
    }

    // Build tics here?
    // TODO: Doom-like timesteps
    timestep.run_this(|_| {
        // G_Ticker
        game.ticker();
    });
}
