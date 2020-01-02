use crate::input::Input;
use crate::GameOptions;
use sdl2::keyboard::Scancode;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::Sdl;
use wad::map::Map;
use wad::Wad;

pub struct Game {
    input: Input,
    canvas: Canvas<Window>,
    running: bool,
    state_changing: bool,
    wad: Wad,
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

        let wad = Wad::new(options.iwad);
        let mut map = Map::new("E1M1".to_owned());
        wad.load_map(&mut map);

        Game {
            input,
            canvas,
            running: true,
            state_changing: false,
            wad,
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

        if self.input.get_key(Scancode::Escape) {
        } else if self.input.get_key(Scancode::Return) {
        } else if !self.input.get_key(Scancode::Escape) && !self.input.get_key(Scancode::Return) {
            self.state_changing = false;
        }
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
        self.canvas.present();
    }

    /// Called by the main loop
    pub fn running(&self) -> bool {
        self.running
    }
}
