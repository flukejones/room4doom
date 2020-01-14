use std::collections::hash_set::HashSet;

use sdl2::event::Event;
use sdl2::keyboard::Scancode as Sc;
use sdl2::mouse::MouseButton as Mb;
use sdl2::EventPump;

/// Fetch all input
pub struct Input {
    pump:        EventPump,
    key_state:   HashSet<Sc>,
    mouse_state: HashSet<Mb>,
    mouse_pos:   (i32, i32),
    quit:        bool,
}

impl Input {
    pub fn new(pump: EventPump) -> Input {
        Input {
            pump,
            key_state: HashSet::new(),
            mouse_state: HashSet::new(),
            mouse_pos: (0, 0),
            quit: false,
        }
    }

    /// The way this is set up to work is that for each `game tick`, a fresh set of event is
    /// gathered and stored. Then for that single game tick, every part of the game can ask
    /// `Input` for results without the results being removed.
    ///
    /// The results of the `update` are valid until the next `update` whereupon they are refreshed.
    ///
    /// **rust-sdl2** provides an `event_iter()`, but this isn't very useful unless we perform
    /// all the required actions in the same block that it is called in. It has the potential
    /// to cause delays in proccessing
    ///
    pub fn update(&mut self) {
        if let Some(event) = self.pump.poll_event() {
            match event {
                Event::KeyDown { .. } => {
                    self.key_state = self
                        .pump
                        .keyboard_state()
                        .pressed_scancodes()
                        .collect();
                }
                Event::KeyUp { .. } => {
                    self.key_state = self
                        .pump
                        .keyboard_state()
                        .pressed_scancodes()
                        .collect();
                }

                Event::MouseButtonDown { mouse_btn, .. } => {
                    self.mouse_state.insert(mouse_btn);
                }
                Event::MouseButtonUp { mouse_btn, .. } => {
                    self.mouse_state.remove(&mouse_btn);
                }

                Event::Quit { .. } => self.quit = true, // Early out if Quit

                Event::MouseMotion { x, y, .. } => {
                    self.mouse_pos.0 = x;
                    self.mouse_pos.1 = y;
                }
                _ => {}
            }
        }
    }

    pub fn get_key(&self, s: Sc) -> bool {
        self.key_state.contains(&s)
    }

    pub fn get_mbtn(&self, m: Mb) -> bool {
        self.mouse_state.contains(&m)
    }

    pub fn get_mpos(&self) -> (i32, i32) {
        self.mouse_pos
    }

    pub fn get_quit(&self) -> bool {
        self.quit
    }
}
