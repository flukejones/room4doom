use std::{
    error::Error,
    f32::consts::{FRAC_PI_2, FRAC_PI_3, FRAC_PI_4, PI},
    fmt,
    str::FromStr,
};

use glam::{Mat4, Vec3};
use golem::Dimension::*;
use golem::*;

use gumdrop::Options;
use sdl2::{
    keyboard::Scancode, pixels::Color, pixels::PixelFormatEnum, rect::Rect,
    render::Canvas, surface::Surface, video::Window,
};

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

pub fn d_doom_loop(mut game: Game, mut input: Input, gl: Window, ctx: Context) {
    let mut timestep = TimeStep::new();

    let mut render_buffer = Surface::new(320, 200, PixelFormatEnum::RGB24)
        .unwrap()
        .into_canvas()
        .unwrap();
    let mut final_buffer = Surface::new(640, 400, PixelFormatEnum::RGB24)
        .unwrap()
        .into_canvas()
        .unwrap();
    let texture_creator = final_buffer.texture_creator();

    // TODO: sort this block of stuff out
    let wsize = gl.drawable_size();
    let ratio = wsize.1 as f32 / 3.0;
    let xw = ratio * 4.0;
    let xp = (wsize.0 as f32 - xw) / 2.0;
    game.crop_rect = Rect::new(xp as i32, 0, xw as u32, wsize.1);

    ctx.set_viewport(
        game.crop_rect.x() as u32,
        game.crop_rect.y() as u32,
        game.crop_rect.width(),
        game.crop_rect.height(),
    );

    let scale = false;
    let mut rend = FinalRenderer::new(&ctx);
    rend.set_tex_filter().unwrap();

    loop {
        if !game.running() {
            break;
        }

        render_buffer.set_draw_color(Color::RGB(0, 0, 0));
        render_buffer.clear();

        // Update the game state
        try_run_tics(&mut game, &mut input, &mut timestep);
        // TODO: S_UpdateSounds(players[consoleplayer].mo); // move positional sounds
        // Draw everything to the buffer
        d_display(&mut game, &mut render_buffer);

        // So many read/writes!
        // Throw it through the final render pass for shaders etc
        if scale {
            let texture = texture_creator
                .create_texture_from_surface(render_buffer.surface())
                .unwrap();
            final_buffer.copy(&texture, None, None).unwrap();
        }

        if scale {
            let pix = final_buffer
                .read_pixels(Rect::new(0, 0, 640, 400), PixelFormatEnum::RGB24)
                .unwrap();
            rend.draw(&ctx, &pix, (640, 400)).unwrap();
        } else {
            let pix = render_buffer
                .read_pixels(Rect::new(0, 0, 320, 200), PixelFormatEnum::RGB24)
                .unwrap();
            rend.draw(&ctx, &pix, (320, 200)).unwrap();
        };
        // Showtime!
        gl.gl_swap_window();
    }
}

/// D_Display
/// Does a bunch of stuff in Doom...
pub fn d_display(game: &mut Game, mut canvas: &mut Canvas<Surface>) {
    //if (gamestate == GS_LEVEL && !automapactive && gametic)
    game.render_player_view(&mut canvas);
    //canvas.present();

    // // menus go directly to the screen
    // TODO: M_Drawer();	 // menu is drawn even on top of everything
    // net update does i/o and buildcmds...
    // TODO: NetUpdate(); // send out any new accumulation
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

struct FinalRenderer {
    _quad:      [f32; 16],
    indices:    [u32; 6],
    shader:     ShaderProgram,
    projection: Mat4,
    look_at:    Mat4,
    texture:    Texture,
    vb:         VertexBuffer,
    eb:         ElementBuffer,
}
impl FinalRenderer {
    fn new(ctx: &Context) -> Self {
        #[rustfmt::skip]
        let quad = [
            // position         vert_uv
            -1.0, -1.0,         0.0, 1.0, // bottom left
            1.0, -1.0,          1.0, 1.0, // bottom right
            1.0, 1.0,           1.0, 0.0, // top right
            -1.0, 1.0,          0.0, 0.0, // top left
        ];
        let indices = [0, 1, 2, 2, 3, 0];

        let shader = ShaderProgram::new(
            ctx,
            ShaderDescription {
                vertex_input:    &[
                    Attribute::new("position", AttributeType::Vector(D2)),
                    Attribute::new("vert_uv", AttributeType::Vector(D2)),
                ],
                fragment_input:  &[Attribute::new(
                    "frag_uv",
                    AttributeType::Vector(D2),
                )],
                uniforms:        &[
                    Uniform::new("projMat", UniformType::Matrix(D4)),
                    Uniform::new("viewMat", UniformType::Matrix(D4)),
                    Uniform::new("modelMat", UniformType::Matrix(D4)),
                    Uniform::new("image", UniformType::Sampler2D),
                ],
                vertex_shader:   r#" void main() {
                                    gl_Position = projMat * viewMat * modelMat * vec4(position, 0.0, 1.0);
                                    frag_uv = vert_uv;
                                }"#,
                fragment_shader: r#" void main() {
                                    vec4 colour = texture(image, frag_uv);
                                    gl_FragColor = colour;
                                }"#,
            },
        ).unwrap();

        let projection = Mat4::perspective_rh_gl(FRAC_PI_4, 1.0, 0.1, 50.0);
        let look_at = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 2.5),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );

        let mut vb = VertexBuffer::new(ctx).unwrap();
        let mut eb = ElementBuffer::new(ctx).unwrap();
        vb.set_data(&quad);
        eb.set_data(&indices);

        Self {
            _quad: quad,
            indices,
            shader,
            projection,
            look_at,
            texture: Texture::new(ctx).unwrap(),
            vb,
            eb,
        }
    }

    fn set_tex_filter(&mut self) -> Result<(), GolemError> {
        self.texture.set_minification(TextureFilter::Nearest)?;
        self.texture.set_magnification(TextureFilter::Linear)
    }

    fn draw(
        &mut self,
        ctx: &Context,
        tex: &[u8],
        size: (u32, u32),
    ) -> Result<(), GolemError> {
        self.texture
            .set_image(Some(tex), size.0, size.1, ColorFormat::RGB);

        self.shader.bind();

        self.shader.set_uniform("image", UniformValue::Int(1))?;

        self.shader.set_uniform(
            "projMat",
            UniformValue::Matrix4(self.projection.to_cols_array()),
        )?;
        self.shader.set_uniform(
            "viewMat",
            UniformValue::Matrix4(self.look_at.to_cols_array()),
        )?;
        self.shader.set_uniform(
            "modelMat",
            UniformValue::Matrix4(Mat4::identity().to_cols_array()),
        )?;

        let bind_point = std::num::NonZeroU32::new(1).unwrap();
        self.texture.set_active(bind_point);

        ctx.clear();
        unsafe {
            self.shader.draw(
                &self.vb,
                &self.eb,
                0..self.indices.len(),
                GeometryMode::Triangles,
            )?;
        }
        Ok(())
    }
}