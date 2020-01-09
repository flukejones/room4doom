use crate::flags::LineDefFlags;
use crate::input::Input;
use crate::GameOptions;
use rand::prelude::*;
use sdl2::gfx::primitives::DrawRenderer;
use sdl2::pixels::Color;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::Sdl;
use std::f32::consts::PI;
use wad::lumps::{Segment, Vertex};
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
        let yel = sdl2::pixels::Color::RGBA(255, 255, 50, 255);
        let blu = sdl2::pixels::Color::RGBA(100, 255, 255, 255);
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
            } else if linedef.flags & LineDefFlags::Secret as u16 == LineDefFlags::Secret as u16 {
                blu
            } else if linedef.line_type != 0 {
                yel
            } else {
                grey
            };
            self.canvas
                .thick_line(start.0, start.1, end.0, end.1, 1, draw_colour)
                .unwrap();
        }

        let player = &self.map.get_things()[0];
        // get the player direction unit vector
        let (py, px) = (player.angle as f32).sin_cos();

        let nodes = self.map.get_nodes();
        self.draw_sector_search(&player.pos, (nodes.len() - 1) as u16, nodes);

        let player = self.vertex_to_screen(&player.pos);
        self.canvas
            .filled_circle(player.0, player.1, 3, yel)
            .unwrap();

        self.canvas
            .thick_line(
                player.0,
                player.1,
                player.0 + (px.ceil() * 25.0) as i16,
                player.1 - (py.ceil() * 25.0) as i16,
                1,
                red,
            )
            .unwrap();
    }

    /// Testing function
    fn draw_line(&self, seg: &Segment) {
        let player = &self.map.get_things()[0].pos;

        let grn = sdl2::pixels::Color::RGBA(100, 255, 100, 255);
        let grey = sdl2::pixels::Color::RGBA(120, 120, 120, 255);

        // lines have direction, which means the angle can tell us if
        // the seg is back facing
        // TODO: make this a func (R_PointToAngle is relative to the player always)
        let angle1 = ((seg.start_vertex.get().x - player.y) as f32)
            .atan2((seg.start_vertex.get().x - player.x) as f32);
        let angle2 = ((seg.end_vertex.get().x - player.y) as f32)
            .atan2((seg.end_vertex.get().x - player.x) as f32);

        if (angle1 - angle2).is_sign_negative() {
            let start = self.vertex_to_screen(seg.start_vertex.get());
            let end = self.vertex_to_screen(seg.end_vertex.get());
            self.canvas
                .thick_line(start.0, start.1, end.0, end.1, 3, grey)
                .unwrap();
            return;
        }

        let start = self.vertex_to_screen(seg.start_vertex.get());
        let end = self.vertex_to_screen(seg.end_vertex.get());
        self.canvas
            .thick_line(start.0, start.1, end.0, end.1, 3, grn)
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
                self.draw_line(&segs[i as usize]);
            }
            return;
        }

        let node = &nodes[node_id as usize];
        let right_box_start = self.vertex_to_screen(&node.bounding_boxes[0][0]);
        let right_box_end = self.vertex_to_screen(&node.bounding_boxes[0][1]);

        self.canvas
            .rectangle(
                right_box_start.0,
                right_box_start.1,
                right_box_end.0,
                right_box_end.1,
                sdl2::pixels::Color::RGBA(42, 100, 42, 255),
            )
            .unwrap();

        let side = node.point_on_side(&v);
        self.draw_sector_search(&v, node.child_index[side], nodes);

        // shortcut if player is in the bounding box
        if node.point_in_bounds(&v, side ^ 1) {
            self.draw_sector_search(&v, node.child_index[side ^ 1], nodes);
        }

        // check if each corner of the BB is in the FOV
        if node.bb_extents_in_fov(&v, 90.0 * PI / 180.0, side ^ 1) {
            self.draw_sector_search(&v, node.child_index[side ^ 1], nodes);
        }
    }
}

/*
NOTES:
1. get angle from player position to bounding box extents
2. check clip list against viewport angle - angle of point
*/
