//! SDL2 input backend — polls `EventPump` for keyboard, mouse, and window
//! events.

use gamestate_traits::{KeyCode, MouseBtn};
use sdl2::EventPump;
use sdl2::event::Event;
use sdl2::keyboard::Scancode as Sc;
use sdl2::mouse::MouseButton as Mb;

use crate::config::InputConfigResolved;
use crate::{InputState, RawEvent};

/// Convert an SDL2 scancode to the backend-agnostic `KeyCode`.
fn sdl_scancode_to_keycode(sc: Sc) -> Option<KeyCode> {
    KeyCode::from_i32(sc as i32)
}

/// Convert an SDL2 mouse button to the backend-agnostic `MouseBtn`.
fn sdl_mousebutton_to_mousebtn(mb: Mb) -> Option<MouseBtn> {
    MouseBtn::from_u8(mb as u8)
}

/// SDL2 input: wraps an `EventPump` and the shared `InputState`.
pub struct InputSdl2 {
    pump: EventPump,
    pub state: InputState,
}

impl InputSdl2 {
    /// Create from an SDL2 event pump and resolved config.
    pub fn new(mut pump: EventPump, config: InputConfigResolved) -> Self {
        pump.pump_events();
        Self {
            pump,
            state: InputState::new(config),
        }
    }

    /// Poll all pending SDL2 events and translate to backend-agnostic types.
    ///
    /// `input_callback` is called on key-down for menu/cheat consumption.
    /// `events_callback` receives non-input window events (resize, etc.).
    pub fn update(
        &mut self,
        mut input_callback: impl FnMut(KeyCode) -> bool,
        mut events_callback: impl FnMut(RawEvent),
    ) {
        while let Some(event) = self.pump.poll_event() {
            match event {
                Event::KeyDown {
                    scancode: Some(sc),
                    ..
                } => {
                    if let Some(kc) = sdl_scancode_to_keycode(sc) {
                        if input_callback(kc) {
                            self.state.events.unset_kb(kc);
                        } else {
                            self.state.events.set_kb(kc);
                        }
                    }
                }
                Event::KeyUp {
                    scancode: Some(sc),
                    ..
                } => {
                    if let Some(kc) = sdl_scancode_to_keycode(sc) {
                        self.state.events.unset_kb(kc);
                    }
                }
                Event::MouseButtonDown {
                    mouse_btn,
                    ..
                } => {
                    if let Some(mb) = sdl_mousebutton_to_mousebtn(mouse_btn) {
                        self.state.events.set_mb(mb);
                    }
                }
                Event::MouseButtonUp {
                    mouse_btn,
                    ..
                } => {
                    if let Some(mb) = sdl_mousebutton_to_mousebtn(mouse_btn) {
                        self.state.events.unset_mb(mb);
                    }
                }
                Event::MouseMotion {
                    xrel,
                    yrel,
                    ..
                } => {
                    let xrel = self.state.events.apply_mouse_accel(xrel as f32) as i32;
                    let yrel = self.state.events.apply_mouse_accel(yrel as f32) as i32;
                    self.state.events.apply_mouse_sensitivity((xrel, yrel));
                }
                Event::Window {
                    win_event,
                    ..
                } => {
                    if matches!(win_event, sdl2::event::WindowEvent::SizeChanged(..)) {
                        events_callback(RawEvent::Resized);
                    }
                }
                Event::Quit {
                    ..
                } => self.state.quit = true,
                _ => {}
            }
        }
    }

    /// Whether the user requested quit.
    pub fn get_quit(&self) -> bool {
        self.state.quit
    }
}
