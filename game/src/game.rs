use crate::flags::LineDefFlags;
use crate::input::Input;
use crate::GameOptions;
use rand::prelude::*;
use sdl2::gfx::primitives::DrawRenderer;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::Sdl;
use wad::lumps::Vertex;
use wad::map::Map;
use wad::Wad;

pub struct Game {
    input: Input,
    canvas: Canvas<Window>,
    running: bool,
    _state_changing: bool,
    _wad: Wad,
    map: Map,
    colours: Vec<Color>,
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
        let map_width = map.get_extents().width;
        let map_height = map.get_extents().height;
        let scr_height = options.height.unwrap_or(200);
        let scr_width = options.width.unwrap_or(320);
        if scr_height > scr_width {
            map.set_scale(map_height.abs() / scr_width as i16);
        } else {
            map.set_scale(map_width.abs() / scr_height as i16);
        }
        dbg!(map.get_extents().automap_scale);

        let mut rng = rand::thread_rng();
        let mut colours = Vec::new();
        for _ in 0..1024 {
            colours.push(sdl2::pixels::Color::RGBA(
                rng.gen_range(50, 255),
                rng.gen_range(50, 255),
                rng.gen_range(50, 255),
                255,
            ));
        }

        Game {
            input,
            canvas,
            running: true,
            _state_changing: false,
            _wad: wad,
            map,
            colours,
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

    fn vertex_to_screen(&self, v: &Vertex) -> (i16, i16) {
        let scale = self.map.get_extents().automap_scale;
        let scr_height = self.canvas.viewport().height() as i16;
        let scr_width = self.canvas.viewport().width() as i16;

        let x_pad = (scr_width * scale - self.map.get_extents().width) / 2;
        let y_pad = (scr_height * scale - self.map.get_extents().height) / 2;

        let x_shift = -(self.map.get_extents().min_vertex.x) + x_pad;
        let y_shift = -(self.map.get_extents().min_vertex.y) + y_pad;
        (
            ((v.x + x_shift) / scale),
            (scr_height - (v.y + y_shift) / scale),
        )
    }

    /// This is really just a test function
    pub fn draw_automap(&mut self) {
        let red = sdl2::pixels::Color::RGBA(255, 100, 100, 255);
        let grn = sdl2::pixels::Color::RGBA(100, 255, 100, 255);
        let grey = sdl2::pixels::Color::RGBA(100, 100, 100, 255);
        let black = sdl2::pixels::Color::RGBA(0, 0, 0, 255);
        // clear background to black
        self.canvas.set_draw_color(black);
        self.canvas.clear();

        for linedef in self.map.get_linedefs() {
            let start = self.vertex_to_screen(linedef.start_vertex.get());
            let end = self.vertex_to_screen(linedef.end_vertex.get());
            let draw_colour = if linedef.flags & LineDefFlags::TwoSided as u16 == 0 {
                red
            } else {
                grey
            };
            self.canvas
                .thick_line(start.0, start.1, end.0, end.1, 1, draw_colour)
                .unwrap();
        }

        for (i, thing) in self.map.get_things().iter().enumerate() {
            let v = self.vertex_to_screen(&thing.pos);
            self.canvas
                .filled_circle(v.0, v.1, 1, self.colours[i])
                .unwrap();
        }

        let segs = self.map.get_segments();
        for (i, subsect) in self.map.get_subsectors().iter().enumerate() {
            let count = subsect.seg_count;
            let mut x_a: Vec<i16> = Vec::new();
            let mut y_a: Vec<i16> = Vec::new();
            for s in subsect.start_seg..subsect.start_seg + count {
                if let Some(seg) = segs.get(s as usize) {
                    let start = self.vertex_to_screen(seg.start_vertex.get());
                    let end = self.vertex_to_screen(seg.end_vertex.get());
                    self.canvas
                        .thick_line(start.0, start.1, end.0, end.1, 1, self.colours[i as usize])
                        .unwrap();
                    self.canvas
                        .filled_circle(start.0, start.1, 3, self.colours[i as usize])
                        .unwrap();
                    self.canvas
                        .filled_circle(end.0, end.1, 2, self.colours[i as usize])
                        .unwrap();
                }
            }
        }

        // Show root node boxes and splitter
        let node = &self.map.get_nodes()[self.map.get_nodes().len() - 1];
        let split_start = self.vertex_to_screen(&node.split_start);
        let right_box_start = self.vertex_to_screen(&node.right_box_start);
        let right_box_end = self.vertex_to_screen(&node.right_box_end);
        let left_box_start = self.vertex_to_screen(&node.left_box_start);
        let left_box_end = self.vertex_to_screen(&node.left_box_end);

        self.canvas
            .rectangle(
                right_box_start.0,
                right_box_start.1,
                right_box_end.0,
                right_box_end.1,
                grn,
            )
            .unwrap();
        self.canvas
            .rectangle(
                left_box_start.0,
                left_box_start.1,
                left_box_end.0,
                left_box_end.1,
                red,
            )
            .unwrap();

        let scr_height = self.canvas.viewport().height() as i16;
        for node in self.map.get_nodes() {
            let split_start = self.vertex_to_screen(&node.split_start);
            self.canvas
                .thick_line(
                    split_start.0,
                    split_start.1,
                    split_start.0 + &node.split_change.x / self.map.get_extents().automap_scale,
                    (split_start.1
                        + -(&node.split_change.y / self.map.get_extents().automap_scale)),
                    3,
                    sdl2::pixels::Color::RGBA(255, 255, 255, 255),
                )
                .unwrap();
        }
    }
}
