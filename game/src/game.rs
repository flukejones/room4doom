use crate::entities::Player;
use crate::flags::LineDefFlags;
use crate::input::Input;
use crate::GameOptions;
use sdl2::gfx::primitives::DrawRenderer;
use sdl2::keyboard::Scancode;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::Sdl;
use std::f32::consts::{FRAC_PI_4, PI};
use vec2d::{radian_range, Vec2d};
use wad::map::Map;
use wad::nodes::{Node, IS_SSECTOR_MASK};
use wad::Wad;
use wad::{lumps::Segment, Vertex};

pub struct Game {
    input: Input,
    canvas: Canvas<Window>,
    running: bool,
    _state_changing: bool,
    _wad: Wad,
    map: Map,
    player: Player,
}

impl Game {
    /// On `Game` object creation, initialize all the game subsystems where possible
    ///
    /// Ideally full error checking will be done in by system.
    ///
    pub fn new(sdl_ctx: &mut Sdl, options: GameOptions) -> Game {
        let video_ctx = sdl_ctx.video().unwrap();
        // Create a window
        let window: Window = video_ctx
            .window(
                "Game Framework",
                options.width.unwrap_or(320),
                options.height.unwrap_or(200),
            )
            .position_centered()
            .opengl()
            .build()
            .unwrap();

        let canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .unwrap();

        let events = sdl_ctx.event_pump().unwrap();

        let input = Input::new(events);

        let mut wad = Wad::new(options.iwad);
        wad.read_directories();
        let mut map = Map::new(options.map.unwrap_or("E1M1".to_owned()));
        wad.load_map(&mut map);

        // options.width.unwrap_or(320) as i16 / options.height.unwrap_or(200) as i16
        let map_width = map.get_extents().width as f32;
        let map_height = map.get_extents().height as f32;
        let scr_height = options.height.unwrap_or(200) as f32;
        let scr_width = options.width.unwrap_or(320) as f32;
        if map_height > map_width {
            map.set_scale(map_height / scr_height * 1.1);
        } else {
            map.set_scale(map_width / scr_width * 1.4);
        }

        let player_thing = &map.get_things()[0];
        let player = Player::new(player_thing.pos.clone(), player_thing.angle * PI / 180.0);

        Game {
            input,
            canvas,
            running: true,
            _state_changing: false,
            _wad: wad,
            map,
            player,
        }
    }

    /// Called by the main loop
    pub fn update(&mut self, time: f64) {
        self.running = !self.input.get_quit();

        if self.input.get_key(Scancode::Escape) {
            self.running = false;
        }

        if self.input.get_key(Scancode::Left) {
            self.player.set_r(radian_range(self.player.rot() + 0.01));
        }

        if self.input.get_key(Scancode::Right) {
            self.player.set_r(radian_range(self.player.rot() - 0.01));
        }

        if self.input.get_key(Scancode::Up) {
            let heading = self.player.rot().sin_cos();
            self.player.set_x(self.player.pos().x + heading.1 * 2.0);
            self.player.set_y(self.player.pos().y + heading.0 * 2.0);
        }

        if self.input.get_key(Scancode::Down) {
            let heading = self.player.rot().sin_cos();
            self.player.set_x(self.player.pos().x - heading.1 * 2.0);
            self.player.set_y(self.player.pos().y - heading.0 * 2.0);
        }
    }

    /// `handle_events` updates the current events and inputs plus changes `states`
    ///
    /// In an C++ engine, using a button to switch states would probably be
    /// handled in the state itself. We can't do that with rust as it requires
    /// passing a mutable reference to the state machine to the state; essentially
    /// this is the same as an object in an Vec<Type> trying to modify its container.
    ///
    /// So because of the above reasons, `states::States` does not allow a game state
    /// to handle state changes or fetching
    ///
    pub fn handle_events(&mut self) {
        self.input.update();

        if self.input.get_key(Scancode::Escape) {}
        //        } else if self.input.get_key(Scancode::Return) {
        //        } else if !self.input.get_key(Scancode::Escape) && !self.input.get_key(Scancode::Return) {
        //            self.state_changing = false;
        //        }
    }

    /// `render` calls the `states.render()` method with a time-step for state renders
    ///
    /// The main loop, in this case, the `'running : loop` in lib.rs should calculate
    /// a time-step to pass down through the render functions for use in the game states,
    /// from which the game states (or menu) will use to render objects at the correct
    /// point in time.
    ///
    pub fn render(&mut self, dt: f64) {
        // The state machine will handle which state renders to the surface
        //self.states.render(dt, &mut self.canvas);
        self.draw_automap();
        self.canvas.present();
    }

    /// Called by the main loop
    pub fn running(&self) -> bool {
        self.running
    }

    fn vertex_to_screen(&self, v: &Vertex) -> (i16, i16) {
        let scale = self.map.get_extents().automap_scale;
        let scr_height = self.canvas.viewport().height() as f32;
        let scr_width = self.canvas.viewport().width() as f32;

        let x_pad = (scr_width * scale - self.map.get_extents().width) / 2.0;
        let y_pad = (scr_height * scale - self.map.get_extents().height) / 2.0;

        let x_shift = -(self.map.get_extents().min_vertex.x) as f32 + x_pad;
        let y_shift = -(self.map.get_extents().min_vertex.y) as f32 + y_pad;
        (
            ((v.x as f32 + x_shift) / scale) as i16,
            (scr_height - (v.y as f32 + y_shift) / scale) as i16,
        )
    }

    /// Testing function
    pub fn draw_automap(&mut self) {
        let yel = sdl2::pixels::Color::RGBA(255, 255, 50, 255);
        // clear background to black
        self.canvas
            .set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 255));
        self.canvas.clear();

        for linedef in self.map.get_linedefs() {
            let start = self.vertex_to_screen(linedef.start_vertex.get());
            let end = self.vertex_to_screen(linedef.end_vertex.get());
            let draw_colour = if linedef.flags & LineDefFlags::TwoSided as u16 == 0 {
                sdl2::pixels::Color::RGBA(255, 100, 100, 255)
            } else if linedef.flags & LineDefFlags::Secret as u16 == LineDefFlags::Secret as u16 {
                sdl2::pixels::Color::RGBA(100, 255, 255, 255)
            } else if linedef.line_type != 0 {
                yel
            } else {
                sdl2::pixels::Color::RGBA(148, 148, 148, 255)
            };
            self.canvas
                .thick_line(start.0, start.1, end.0, end.1, 1, draw_colour)
                .unwrap();
        }

        let nodes = self.map.get_nodes();
        //self.draw_sector_search(&self.player.pos(), (nodes.len() - 1) as u16, nodes);
        let mut segs = Vec::new();
        self.map.list_segs_facing_point(
            &self.player.pos(),
            self.player.rot(),
            (nodes.len() - 1) as u16,
            &mut segs,
        );
        for seg in segs {
            self.draw_line(seg);
        }

        let player = self.vertex_to_screen(&self.player.pos());

        let (py, px) = self.player.rot().sin_cos();
        let (lpy, lpx) = (self.player.rot() + PI / 4.0).sin_cos();
        let (rpy, rpx) = (self.player.rot() - PI / 4.0).sin_cos();
        self.canvas
            .thick_line(
                player.0,
                player.1,
                player.0 + (px * 25.0) as i16,
                player.1 - (py * 25.0) as i16,
                2,
                yel,
            )
            .unwrap();
        self.canvas
            .thick_line(
                player.0,
                player.1,
                player.0 + (lpx * 500.0) as i16,
                player.1 - (lpy * 500.0) as i16,
                2,
                yel,
            )
            .unwrap();
        self.canvas
            .thick_line(
                player.0,
                player.1,
                player.0 + (rpx * 500.0) as i16,
                player.1 - (rpy * 500.0) as i16,
                2,
                yel,
            )
            .unwrap();
    }

    /// Testing function
    fn draw_line(&self, seg: &Segment) {
        let player = self.player.pos();
        let screen_start = self.vertex_to_screen(seg.start_vertex.get());
        let screen_end = self.vertex_to_screen(seg.end_vertex.get());

        let mut draw_colour = sdl2::pixels::Color::RGBA(0, 160, 0, 125);

        let flags = seg.linedef.get().flags;
        if flags & LineDefFlags::TwoSided as u16 == 0 {
            draw_colour = sdl2::pixels::Color::RGBA(0, 255, 0, 105);
        }

        if seg.linedef.get().flags & LineDefFlags::Secret as u16 == LineDefFlags::Secret as u16 {
            draw_colour = sdl2::pixels::Color::RGBA(100, 255, 255, 255);
        }

        // Because linedefs may have multiple segs in both directions we'll mask out some
        // just for the sake of drawing
        if seg.direction == 0 {
            self.canvas
                .thick_line(
                    screen_start.0,
                    screen_start.1,
                    screen_end.0,
                    screen_end.1,
                    3,
                    draw_colour,
                )
                .unwrap();
        }
    }
}
