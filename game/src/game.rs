use crate::flags::LineDefFlags;
use crate::input::Input;
use crate::GameOptions;
use sdl2::gfx::primitives::DrawRenderer;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::Sdl;
use wad::lumps::Vertex;
use wad::map::Map;
use wad::nodes::{Node, IS_SSECTOR_MASK};
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

        Game {
            input,
            canvas,
            running: true,
            _state_changing: false,
            _wad: wad,
            map,
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
        let red = sdl2::pixels::Color::RGBA(255, 100, 100, 255);
        let grn = sdl2::pixels::Color::RGBA(100, 255, 100, 255);
        let yel = sdl2::pixels::Color::RGBA(255, 255, 100, 255);
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

        let player = &self.map.get_things()[0].pos;
        let nodes = self.map.get_nodes();
        self.draw_sector_search(player, (nodes.len() - 1) as u16, nodes);

        let player = self.vertex_to_screen(player);
        self.canvas
            .filled_circle(player.0, player.1, 3, yel)
            .unwrap();
    }

    /// Testing function. Mostly trying out different ways to present information from the BSP
    fn draw_sector_search(&self, v: &Vertex, node_id: u16, nodes: &[Node]) {
        let draw_seg = |sect_id: u16| {};

        if node_id & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
            // It's a leaf node and is the index to a subsector
            let subsect = &self.map.get_subsectors()[(node_id ^ IS_SSECTOR_MASK) as usize];
            let segs = self.map.get_segments();

            for i in subsect.start_seg..subsect.start_seg + subsect.seg_count {
                let start = self.vertex_to_screen(&segs[i as usize].start_vertex.get());
                let end = self.vertex_to_screen(&segs[i as usize].end_vertex.get());
                self.canvas
                    .thick_line(
                        start.0,
                        start.1,
                        end.0,
                        end.1,
                        3,
                        sdl2::pixels::Color::RGBA(42, 255, 42, 255),
                    )
                    .unwrap();
            }
            return;
        }

        let node = &nodes[node_id as usize];
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
                sdl2::pixels::Color::RGBA(42, 100, 42, 255),
            )
            .unwrap();

        let dx = (v.x - nodes[node_id as usize].split_start.x) as i32;
        let dy = (v.y - nodes[node_id as usize].split_start.y) as i32;

        if (dx * nodes[node_id as usize].split_change.y as i32)
            - (dy * nodes[node_id as usize].split_change.x as i32)
            <= 0
        {
            self.draw_sector_search(&v, nodes[node_id as usize].left_child_id, nodes);
        } else {
            self.draw_sector_search(&v, nodes[node_id as usize].right_child_id, nodes);
        }
    }
}

/*
NOTES:
0. This is a shortcut only. We need to use viewport angle and bounds box to
   determine if walls are in scope
   if v.x.abs() > nodes[node_id as usize].right_box_start.x.abs()
                && v.x.abs() < nodes[node_id as usize].right_box_end.x.abs()
                && v.y.abs() > nodes[node_id as usize].right_box_start.y.abs()
                && v.y.abs() < nodes[node_id as usize].right_box_end.y.abs()
1. get angle from player position to bounding box extents
2. check clip list against viewport angle - angle of point
*/
