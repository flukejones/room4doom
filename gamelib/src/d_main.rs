use std::{error::Error, fmt, str::FromStr};

use gumdrop::Options;
use sdl2::{
    keyboard::Scancode, pixels::PixelFormatEnum, render::Canvas,
    surface::Surface, video::Window,
};

use crate::{input::Input, timestep::TimeStep, Game};

const MS_PER_UPDATE: f32 = 4.0;

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

#[derive(Debug)]
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
    #[options(help = "path to game WAD", default = "./doom1.wad")]
    pub iwad:       String,
    #[options(help = "path to patch WAD")]
    pub pwad:       Option<String>,
    #[options(help = "resolution width in pixels", default = "640")]
    pub width:      u32,
    #[options(help = "resolution height in pixels", default = "480")]
    pub height:     u32,
    #[options(help = "fullscreen?")]
    pub fullscreen: bool,

    #[options(help = "Disable monsters")]
    pub no_monsters:   bool,
    #[options(help = "Monsters respawn after being killed")]
    pub respawn_parm:  bool,
    #[options(help = "Monsters move faster")]
    pub fast_parm:     bool,
    #[options(
        help = "Developer mode. F1 saves a screenshot in the current working directory"
    )]
    pub dev_parm:      bool,
    #[options(
        help = "Start a deathmatch game: 1 = classic, 2 = Start a deathmatch 2.0 game.  Weapons do not stay in place and all items respawn after 30 seconds"
    )]
    pub deathmatch:    u8,
    #[options(
        help = "Set the game skill, 1-5 (1: easiest, 5: hardest). A skill of 0 disables all monsters"
    )]
    pub start_skill:   Skill,
    #[options(help = "Select episode")]
    pub start_episode: u32,
    #[options(help = "Select map in episode")]
    pub start_map:     u32,
    pub autostart:     bool,
}

pub fn d_doom_loop(
    mut game: Game,
    mut input: Input,
    mut canvas: Canvas<Window>,
) {
    let mut timestep = TimeStep::new();
    let mut lag = 0.0;

    'running: loop {
        if !game.running() {
            break 'running;
        }
        // temporary block
        input.update();
        game.set_running(!input.get_quit());
        if input.get_key(Scancode::Escape) {
            game.set_running(false);
        }

        lag += timestep.delta();

        while lag >= MS_PER_UPDATE {
            let time = MS_PER_UPDATE * 0.01;
            // temorary block
            let rot_amnt = 0.15 * time;
            let mv_amnt = 50.0 * time;
            if input.get_key(Scancode::Left) {
                game.players[0].rotation += rot_amnt;
            }

            if input.get_key(Scancode::Right) {
                game.players[0].rotation -= rot_amnt;
            }

            if input.get_key(Scancode::Up) {
                let heading = game.players[0].rotation.sin_cos();
                game.players[0]
                    .xy
                    .set_x(game.players[0].xy.x() + heading.1 * mv_amnt);
                game.players[0]
                    .xy
                    .set_y(game.players[0].xy.y() + heading.0 * mv_amnt);
            }

            if input.get_key(Scancode::Down) {
                let heading = game.players[0].rotation.sin_cos();
                game.players[0]
                    .xy
                    .set_x(game.players[0].xy.x() - heading.1 * mv_amnt);
                game.players[0]
                    .xy
                    .set_y(game.players[0].xy.y() - heading.0 * mv_amnt);
            }

            lag -= MS_PER_UPDATE;
        }

        let surface = Surface::new(320, 200, PixelFormatEnum::RGB555).unwrap();
        let mut drawer = surface.into_canvas().unwrap();
        game.d_display(&mut drawer);
        // consume the canvas
        game.i_finish_update(drawer, &mut canvas);
    }
}

fn d_run_frame(mut game: Game, mut input: Input, mut canvas: Canvas<Window>) {
    // if (wipe)
    // {
    //     do
    //     {
    //         nowtime = I_GetTime();
    //         tics = nowtime - wipestart;
    //         I_Sleep(1);
    //     } while (tics <= 0);

    //     wipestart = nowtime;
    //     wipe = !wipe_ScreenWipe(wipe_Melt, 0, 0, SCREENWIDTH, SCREENHEIGHT, tics);
    //     I_UpdateNoBlit();
    //     M_Drawer();       // menu is drawn even on top of wipes
    //     I_FinishUpdate(); // page flip or blit buffer
    //     return;
    // }

    // // frame syncronous IO operations
    // I_StartFrame();

    // TryRunTics(); // will run at least one tic

    // S_UpdateSounds(players[consoleplayer].mo); // move positional sounds

    // // Update display, next frame, with current state if no profiling is on
    // if (screenvisible && !nodrawers)
    // {
    //     if ((wipe = D_Display()))
    //     {
    //         // start wipe on this frame
    //         wipe_EndScreen(0, 0, SCREENWIDTH, SCREENHEIGHT);

    //         wipestart = I_GetTime() - 1;
    //     }
    //     else
    //     {
    //         // normal update
    //         I_FinishUpdate(); // page flip or blit buffer
    //     }
    // }
}
