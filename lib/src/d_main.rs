use std::{error::Error, fmt, str::FromStr};

use golem::Context;

use gumdrop::Options;
use log::{debug, trace};
use sdl2::{
    keyboard::Scancode, pixels::Color, pixels::PixelFormatEnum, rect::Rect, render::Canvas,
    surface::Surface, video::Window,
};

use crate::{
    doom_def::GameMission,
    doom_def::GameMode,
    game::Game,
    input::Input,
    shaders::{lottes_crt::LottesCRT, Renderer},
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
    Baby = 0,
    Easy,
    Medium,
    Hard,
    Nightmare,
}

impl Default for Skill {
    fn default() -> Self {
        Skill::Medium
    }
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
    #[options(
        help = "verbose level: off, error, warn, info, debug",
        default = "warn"
    )]
    pub verbose: log::LevelFilter,
    #[options(no_short, help = "path to game WAD", default = "./doom1.wad")]
    pub iwad: String,
    #[options(no_short, help = "path to patch WAD")]
    pub pwad: Option<String>,
    #[options(help = "resolution width in pixels", default = "640")]
    pub width: u32,
    #[options(help = "resolution height in pixels", default = "480")]
    pub height: u32,
    #[options(help = "fullscreen?")]
    pub fullscreen: bool,

    #[options(help = "Disable monsters")]
    pub no_monsters: bool,
    #[options(help = "Monsters respawn after being killed")]
    pub respawn_parm: bool,
    #[options(help = "Monsters move faster")]
    pub fast_parm: bool,
    #[options(
        no_short,
        help = "Developer mode. F1 saves a screenshot in the current working directory"
    )]
    pub dev_parm: bool,
    #[options(
        help = "Start a deathmatch game: 1 = classic, 2 = Start a deathmatch 2.0 game.  Weapons do not stay in place and all items respawn after 30 seconds"
    )]
    pub deathmatch: u8,
    #[options(
        help = "Set the game skill, 1-5 (1: easiest, 5: hardest). A skill of 0 disables all monsters"
    )]
    pub skill: Skill,
    #[options(help = "Select episode", default = "1")]
    pub episode: u32,
    #[options(help = "Select level in episode", default = "1")]
    pub map: u32,
    pub autostart: bool,
    #[options(help = "game options help")]
    pub help: bool,
}

pub fn identify_version(wad: &wad::WadData) -> (GameMode, GameMission, String) {
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
            game_description = String::from("The Ultimate DOOM");
        } else if wad.lump_exists("E3M1") {
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

/// Never returns
pub fn d_doom_loop(
    mut game: Game,
    mut input: Input,
    gl: Window,
    ctx: Context,
) -> Result<(), Box<dyn Error>> {
    debug!("Entered d_doom_loop");

    let mut timestep = TimeStep::new();
    let mut render_buffer = Surface::new(320, 200, PixelFormatEnum::RGBA32)?.into_canvas()?;

    // TODO: sort this block of stuff out
    let wsize = gl.drawable_size();
    let ratio = wsize.1 as f32 * 1.333333;
    let xp = (wsize.0 as f32 - ratio) / 2.0;
    game.crop_rect = Rect::new(xp as i32, 0, ratio as u32, wsize.1);

    ctx.set_viewport(
        game.crop_rect.x() as u32,
        game.crop_rect.y() as u32,
        game.crop_rect.width(),
        game.crop_rect.height(),
    );

    //let mut rend = Basic::new(&ctx);
    let mut rend = LottesCRT::new(&ctx);
    //let mut rend = CGWGCRT::new(&ctx, game.crop_rect.width(), game.crop_rect.height());
    rend.set_tex_filter().unwrap();

    loop {
        if !game.running() {
            break;
        }

        render_buffer.set_draw_color(Color::RGBA(15, 0, 0, 0));
        render_buffer.clear();

        // Update the game state
        try_run_tics(&mut game, &mut input, &mut timestep);

        // TODO: S_UpdateSounds(players[consoleplayer].mo); // move positional sounds
        // Draw everything to the buffer
        d_display(&mut game, &mut render_buffer);

        let pix = render_buffer
            .read_pixels(render_buffer.surface().rect(), PixelFormatEnum::RGBA32)
            .unwrap();

        rend.clear();
        rend.set_image_data(&pix, render_buffer.surface().size());
        rend.draw().unwrap();

        gl.gl_swap_window();

        if let Some(fps) = timestep.frame_rate() {
            trace!("{:?}", fps);
        }
    }
    Ok(())
}

/// D_Display
/// Does a bunch of stuff in Doom...
fn d_display(game: &mut Game, canvas: &mut Canvas<Surface>) {
    //if (gamestate == GS_LEVEL && !automapactive && gametic)
    game.render_player_view(canvas);
    //canvas.present();

    // // menus go directly to the screen
    // TODO: M_Drawer();	 // menu is drawn even on top of everything
    // net update does i/o and buildcmds...
    // TODO: NetUpdate(); // send out any new accumulation
}

fn try_run_tics(game: &mut Game, input: &mut Input, timestep: &mut TimeStep) {
    trace!("Entered try_run_tics");
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
