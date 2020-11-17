use std::f32::consts::PI;

use sdl2::{
    keyboard::Scancode, pixels::PixelFormatEnum, render::Canvas,
    surface::Surface, video::Window,
};

use gamelib::{angle::Angle, bsp::Bsp, player::Player};
use wad::{DPtr, Wad};

use crate::input::Input;
use crate::{GameOptions, FP};

pub struct Game<'c> {
    input:           &'c mut Input,
    canvas:          &'c mut Canvas<Window>,
    running:         bool,
    _state_changing: bool,
    _wad:            Wad,
    map:             Bsp,
    player:          Player,
}

impl<'c> Game<'c> {
    /// On `Game` object creation, initialize all the game subsystems where possible
    ///
    /// Ideally full error checking will be done in by system.
    ///
    pub fn new(
        canvas: &'c mut Canvas<Window>,
        input: &'c mut Input,
        options: GameOptions,
    ) -> Game<'c> {
        let mut wad = Wad::new(options.iwad);
        wad.read_directories();
        let mut map = Bsp::new(options.map.unwrap_or("E1M1".to_owned()));
        map.load(&wad);

        let player_thing = &map.get_things()[0];
        let player_subsect = map.find_subsector(&player_thing.pos).unwrap();

        let player = Player::new(
            player_thing.pos.clone(),
            player_subsect.sector.floor_height as f32 + 41.0,
            Angle::new(player_thing.angle * PI / 180.0),
            DPtr::new(player_subsect),
        );

        dbg!(&player);

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
    pub fn update(&mut self, time: FP) {
        let rot_amnt = 0.15 * time;
        let mv_amnt = 50.0 * time;
        if self.input.get_key(Scancode::Left) {
            self.player.rotation += rot_amnt;
        }

        if self.input.get_key(Scancode::Right) {
            self.player.rotation -= rot_amnt;
        }

        if self.input.get_key(Scancode::Up) {
            let heading = self.player.rotation.sin_cos();
            self.player
                .xy
                .set_x(self.player.xy.x() + heading.1 * mv_amnt);
            self.player
                .xy
                .set_y(self.player.xy.y() + heading.0 * mv_amnt);
        }

        if self.input.get_key(Scancode::Down) {
            let heading = self.player.rotation.sin_cos();
            self.player
                .xy
                .set_x(self.player.xy.x() - heading.1 * mv_amnt);
            self.player
                .xy
                .set_y(self.player.xy.y() - heading.0 * mv_amnt);
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
        self.running = !self.input.get_quit();

        if self.input.get_key(Scancode::Escape) {
            self.running = false;
        }
    }

    /// `render` calls the `states.render()` method with a time-step for state renders
    ///
    /// The main loop, in this case, the `'running : loop` in lib.rs should calculate
    /// a time-step to pass down through the render functions for use in the game states,
    /// from which the game states (or menu) will use to render objects at the correct
    /// point in time.
    ///
    pub fn render(&mut self, dt: FP) {
        self.map.clear_clip_segs();

        // The state machine will handle which state renders to the surface
        //self.states.render(dt, &mut self.canvas);
        let player_subsect = self.map.find_subsector(&self.player.xy).unwrap();
        self.player.z = player_subsect.sector.floor_height as f32 + 41.0;
        self.player.sub_sector = DPtr::new(player_subsect);

        let surface = Surface::new(320, 200, PixelFormatEnum::RGB555).unwrap();
        let mut canvas = surface.into_canvas().unwrap();
        canvas.clear();
        self.map
            .draw_bsp(&self.player, self.map.start_node(), &mut canvas);
        canvas.present();

        let texture_creator = self.canvas.texture_creator();
        let t = canvas.into_surface().as_texture(&texture_creator).unwrap();

        self.canvas.copy(&t, None, None).unwrap();
        //self.draw_automap();
        self.canvas.present();
    }

    /// Called by the main loop
    pub fn running(&self) -> bool {
        self.running
    }
}
