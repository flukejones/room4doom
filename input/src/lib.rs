//! All input handling. The output is generally a `TicCmd` used to run
//! inputs in the `Game` in a generalised way.
//!
//! Also does config options for controls.

pub mod config;

use std::collections::hash_set::HashSet;

use config::InputConfigResolved;
use gameplay::WeaponType;
use gameplay::tic_cmd::*;
use gamestate_traits::{KeyCode, MouseBtn};

/// Backend-agnostic non-input events forwarded to the game loop.
#[derive(Debug, Clone, Copy)]
pub enum RawEvent {
    /// Window was resized.
    Resized,
}

#[derive(Default, Clone)]
pub struct InputEvents {
    key_state: HashSet<KeyCode>,
    mouse_state: HashSet<MouseBtn>,
    mouse_delta: (i32, i32),
    mouse_sensitivity: (i32, i32),
    mouse_threshold: f32,
    mouse_acceleration: f32,
    turn_held: u32,
}

impl InputEvents {
    fn new(mouse_scale: (i32, i32)) -> Self {
        let mut i = Self::default();
        i.set_mouse_scale(mouse_scale);
        i.mouse_threshold = 10.0;
        i.mouse_acceleration = 2.0;
        i
    }

    pub fn is_kb_pressed(&self, s: KeyCode) -> bool {
        self.key_state.contains(&s)
    }

    pub fn keys_pressed(&self) -> &HashSet<KeyCode> {
        &self.key_state
    }

    pub fn is_mb_pressed(&self, m: MouseBtn) -> bool {
        self.mouse_state.contains(&m)
    }

    pub fn set_kb(&mut self, b: KeyCode) {
        self.key_state.insert(b);
    }

    pub fn unset_kb(&mut self, b: KeyCode) {
        self.key_state.remove(&b);
    }

    pub fn set_mb(&mut self, b: MouseBtn) {
        self.mouse_state.insert(b);
    }

    pub fn unset_mb(&mut self, b: MouseBtn) {
        self.mouse_state.remove(&b);
    }

    pub fn set_mouse_scale(&mut self, scale: (i32, i32)) {
        self.mouse_sensitivity = scale;
    }

    pub fn reset_mouse_delta(&mut self) {
        self.mouse_delta = (0, 0);
    }

    pub fn apply_mouse_sensitivity(&mut self, state: (i32, i32)) {
        self.mouse_delta = (
            state.0 * (self.mouse_sensitivity.0 + 5),
            state.1 * (self.mouse_sensitivity.1),
        );
    }

    pub const fn apply_mouse_accel(&self, val: f32) -> f32 {
        if val < 0.0 {
            return -self.apply_mouse_accel(-val);
        }

        if val > self.mouse_threshold {
            return (val - self.mouse_threshold) * self.mouse_acceleration + self.mouse_threshold;
        } else {
            return val;
        }
    }

    pub fn build_tic_cmd(&mut self, cfg: &InputConfigResolved) -> TicCmd {
        let mut cmd = TicCmd::default();

        let strafe = self.is_kb_pressed(cfg.key_strafe) || self.is_mb_pressed(cfg.mousebstrafe);
        let speed = if self.is_kb_pressed(cfg.key_speed) {
            1
        } else {
            0
        };

        let mut side = 0;
        let mut forward = 0;

        let turn_right = self.is_kb_pressed(cfg.key_right);
        let turn_left = self.is_kb_pressed(cfg.key_left);

        if turn_left || turn_right {
            self.turn_held += 1;
        } else {
            self.turn_held = 0;
        }

        let turn_speed = if self.turn_held < 6 { 2 } else { speed };

        if strafe {
            if self.is_kb_pressed(cfg.key_right) {
                side += SIDEMOVE[speed];
            }
            if self.is_kb_pressed(cfg.key_left) {
                side -= SIDEMOVE[speed];
            }
        } else {
            if self.is_kb_pressed(cfg.key_right) {
                cmd.angleturn -= ANGLETURN[turn_speed];
            }
            if self.is_kb_pressed(cfg.key_left) {
                cmd.angleturn += ANGLETURN[turn_speed];
            }
        }

        if self.is_kb_pressed(cfg.key_up) {
            forward += FORWARDMOVE[speed];
        }

        if self.is_kb_pressed(cfg.key_down) {
            forward -= FORWARDMOVE[speed];
        }

        if self.is_kb_pressed(cfg.key_straferight) {
            side += SIDEMOVE[speed];
        }

        if self.is_kb_pressed(cfg.key_strafeleft) {
            side -= SIDEMOVE[speed];
        }

        if self.is_kb_pressed(cfg.key_fire) || self.is_mb_pressed(cfg.mousebfire) {
            cmd.buttons |= TIC_CMD_BUTTONS.bt_attack;
        }

        if self.is_kb_pressed(cfg.key_use) {
            cmd.buttons |= TIC_CMD_BUTTONS.bt_use;
        }

        for i in 0..WeaponType::NumWeapons as u8 {
            if let Some(key) = KeyCode::from_i32(30 + i as i32) {
                if self.is_kb_pressed(key) {
                    cmd.buttons |= TIC_CMD_BUTTONS.bt_change;
                    if i == 8 {
                        cmd.buttons |= 2 << TIC_CMD_BUTTONS.bt_weaponshift;
                    } else {
                        cmd.buttons |= i << TIC_CMD_BUTTONS.bt_weaponshift;
                    }
                }
            }
        }

        // Mouse
        if self.is_mb_pressed(cfg.mousebforward) {
            forward += FORWARDMOVE[speed];
        }

        let mousex = self.mouse_delta.0;
        let mousey = self.mouse_delta.1;

        if true {
            // TODO: invert settings
            cmd.lookdir = -(mousey) as i16;
        } else {
            forward += -self.mouse_delta.1;
        }

        if strafe {
            side += mousex * 2;
        } else {
            cmd.angleturn -= (mousex * 0x8) as i16;
        }
        self.reset_mouse_delta();

        forward = forward.clamp(-MAXPLMOVE, MAXPLMOVE);
        side = side.clamp(-MAXPLMOVE, MAXPLMOVE);

        cmd.forwardmove += forward as i8;
        cmd.sidemove += side as i8;

        cmd
    }
}

/// Backend-agnostic input state: events, config, and quit flag.
pub struct InputState {
    pub events: InputEvents,
    pub config: InputConfigResolved,
    pub quit: bool,
}

impl InputState {
    /// Create new input state with defaults.
    pub fn new(config: InputConfigResolved) -> Self {
        Self {
            events: InputEvents::new((5, 1)),
            config,
            quit: false,
        }
    }
}

// ── SDL2 backend ──────────────────────────────────────────────────────

#[cfg(feature = "input-sdl2")]
mod sdl2_input;
#[cfg(feature = "input-sdl2")]
pub use sdl2_input::InputSdl2;

// ── winit backend ─────────────────────────────────────────────────────

#[cfg(feature = "input-winit")]
mod winit_input;
#[cfg(feature = "input-winit")]
pub use winit_input::winit_keycode_to_keycode;
#[cfg(feature = "input-winit")]
pub use winit_input::winit_mousebutton_to_mousebtn;
