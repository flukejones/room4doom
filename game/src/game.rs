use crate::input::Input;
use crate::GameOptions;
use sdl2::gfx::primitives::DrawRenderer;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::Sdl;
use wad::map::{LineDefFlags, Map};
use wad::Wad;

pub struct Game {
    input: Input,
    canvas: Canvas<Window>,
    running: bool,
    _state_changing: bool,
    _wad: Wad,
    map: Map,
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
        let mut map = Map::new("E1M1".to_owned());
        wad.load_map(&mut map);

        // options.width.unwrap_or(320) as i16 / options.height.unwrap_or(200) as i16
        let map_width = map.get_extents().max_vertex.x - map.get_extents().min_vertex.x;
        let map_height = map.get_extents().max_vertex.y - map.get_extents().min_vertex.y;
        if map_height > map_width {
            map.set_scale(map_height / options.height.unwrap_or(200) as i16);
        } else {
            map.set_scale(map_width / options.width.unwrap_or(200) as i16);
        }

        Game {
            input,
            canvas,
            running: true,
            _state_changing: false,
            _wad: wad,
            map: map,
        }
    }

    /// Called by the main loop
    pub fn update(&mut self, time: f64) {
        self.running = !self.input.get_quit();
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

        //        if self.input.get_key(Scancode::Escape) {
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

    /// This is really just a test function
    pub fn draw_automap(&mut self) {
        let red = sdl2::pixels::Color::RGBA(255, 100, 100, 255);
        let grey = sdl2::pixels::Color::RGBA(100, 100, 100, 255);
        let black = sdl2::pixels::Color::RGBA(0, 0, 0, 255);
        // clear background to black
        self.canvas.set_draw_color(black);
        self.canvas.clear();

        let scale = self.map.get_extents().automap_scale;
        let x_shift = -(self.map.get_extents().min_vertex.x - scale);
        let y_shift = -(self.map.get_extents().min_vertex.y - scale);
        let scr_height = self.canvas.viewport().height() as i16;

        for linedef in self.map.get_linedefs() {
            let vertexes = self.map.get_vertexes();
            let start = &vertexes[linedef.start_vertex as usize];
            let end = &vertexes[linedef.end_vertex as usize];
            let draw_colour = if linedef.flags & LineDefFlags::TwoSided as u16 == 0 {
                red
            } else {
                grey
            };
            self.canvas
                .thick_line(
                    (start.x + x_shift) / scale,
                    scr_height - (start.y + y_shift) / scale,
                    (end.x + x_shift) / scale,
                    scr_height - (end.y + y_shift) / scale,
                    1,
                    draw_colour,
                )
                .unwrap();
        }
    }
}
