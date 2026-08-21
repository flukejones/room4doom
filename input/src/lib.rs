//! All input handling. The output is generally a `TicCmd` used to run
//! inputs in the `Game` in a generalised way.
//!
//! Also does config options for controls.

pub mod config;

use std::collections::hash_set::HashSet;

use config::InputConfigResolved;
use game_config::WeaponType;
use game_config::tic_cmd::{ANGLETURN, FORWARDMOVE, MAXPLMOVE, SIDEMOVE, TIC_CMD_BUTTONS, TicCmd};
use gamestate_traits::{KeyCode, MouseBtn};

/// Backend-agnostic non-input events forwarded to the game loop.
#[derive(Debug, Clone, Copy)]
pub enum RawEvent {
    /// Window was resized.
    Resized,
}

/// One tic's worth of motion-controller input, in the terms the command
/// needs: sticks as -1..1, buttons already edge-resolved by whoever read
/// the device.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PadState {
    /// +x right, +y forward, in the player's frame. -1..1.
    pub move_axes: [f32; 2],
    pub fire: bool,
    pub use_: bool,
    pub weapon_next: bool,
    pub weapon_prev: bool,
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
    invert_y: bool,
    /// Motion-controller input for the tic being built; the keyboard and
    /// this fold into one command rather than competing.
    pad: PadState,
    /// Weapon the pad cycled to, since Doom's command carries a weapon
    /// number rather than a "next" bit.
    pad_weapon: u8,
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

    pub fn set_invert_y(&mut self, invert: bool) {
        self.invert_y = invert;
    }

    pub fn reset_mouse_delta(&mut self) {
        self.mouse_delta = (0, 0);
    }

    pub fn apply_mouse_sensitivity(&mut self, state: (i32, i32)) {
        self.mouse_delta = (
            state.0 * (self.mouse_sensitivity.0 + 5),
            state.1 * (self.mouse_sensitivity.1 + 5),
        );
    }

    pub const fn apply_mouse_accel(&self, val: f32) -> f32 {
        if val < 0.0 {
            return -self.apply_mouse_accel(-val);
        }

        if val > self.mouse_threshold {
            (val - self.mouse_threshold) * self.mouse_acceleration + self.mouse_threshold
        } else {
            val
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "screen, input and game-state values deliberately retain their established numeric widths and rounding"
    )]
    #[expect(
        clippy::too_many_lines,
        reason = "keep the complete dispatch or geometry pass together without changing its established ordering"
    )]
    pub fn build_tic_cmd(&mut self, cfg: &InputConfigResolved) -> TicCmd {
        let mut cmd = TicCmd::default();

        let strafe = self.is_kb_pressed(cfg.key_strafe) || self.is_mb_pressed(cfg.mousebstrafe);
        let speed = usize::from(self.is_kb_pressed(cfg.key_speed));

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
            if let Some(key) = KeyCode::from_i32(30 + i as i32)
                && self.is_kb_pressed(key)
            {
                cmd.buttons |= TIC_CMD_BUTTONS.bt_change;
                if i == 8 {
                    cmd.buttons |= 2 << TIC_CMD_BUTTONS.bt_weaponshift;
                } else {
                    cmd.buttons |= i << TIC_CMD_BUTTONS.bt_weaponshift;
                }
            }
        }

        // Mouse
        if self.is_mb_pressed(cfg.mousebforward) {
            forward += FORWARDMOVE[speed];
        }

        let mousex = self.mouse_delta.0;
        let mousey = self.mouse_delta.1;

        let look_y = if self.invert_y { mousey } else { -mousey };
        cmd.lookdir = (look_y * 0x8).clamp(i16::MIN as i32, i16::MAX as i32) as i16;

        if strafe {
            side += mousex * 2;
        } else {
            cmd.angleturn -= (mousex * 0x8) as i16;
        }
        self.reset_mouse_delta();

        // Player frame == gaze frame: the stick needs no rotation.
        let pad = self.pad;
        forward += (pad.move_axes[1] * FORWARDMOVE[1] as f32) as i32;
        side += (pad.move_axes[0] * SIDEMOVE[1] as f32) as i32;
        if pad.fire {
            cmd.buttons |= TIC_CMD_BUTTONS.bt_attack;
        }
        if pad.use_ {
            cmd.buttons |= TIC_CMD_BUTTONS.bt_use;
        }
        if pad.weapon_next || pad.weapon_prev {
            let last = WeaponType::NumWeapons as u8 - 1;
            self.pad_weapon = if pad.weapon_next {
                if self.pad_weapon >= last {
                    0
                } else {
                    self.pad_weapon + 1
                }
            } else if self.pad_weapon == 0 {
                last
            } else {
                self.pad_weapon - 1
            };
            cmd.buttons |= TIC_CMD_BUTTONS.bt_change;
            cmd.buttons |= self.pad_weapon << TIC_CMD_BUTTONS.bt_weaponshift;
        }
        // Edges are spent by the tic that saw them; levels stay until
        // the next reading replaces them.
        self.pad.weapon_next = false;
        self.pad.weapon_prev = false;

        forward = forward.clamp(-MAXPLMOVE, MAXPLMOVE);
        side = side.clamp(-MAXPLMOVE, MAXPLMOVE);

        cmd.forwardmove += forward as i8;
        cmd.sidemove += side as i8;

        cmd
    }

    /// Hands a motion-controller reading to the next
    /// [`Self::build_tic_cmd`].
    ///
    /// Called once per rendered frame, which is faster than the tic rate,
    /// so the two kinds of input merge differently. Sticks and held
    /// buttons describe the present, so the newest reading wins. A weapon
    /// change is an edge and must survive until a tic spends it —
    /// overwriting those loses most of them.
    pub fn set_pad(&mut self, pad: PadState) {
        self.pad.move_axes = pad.move_axes;
        self.pad.fire = pad.fire;
        self.pad.use_ = pad.use_;
        self.pad.weapon_next |= pad.weapon_next;
        self.pad.weapon_prev |= pad.weapon_prev;
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

#[cfg(feature = "input-sdl2")]
mod sdl2_input;
#[cfg(feature = "input-sdl2")]
pub use sdl2_input::InputSdl2;

#[cfg(feature = "input-winit")]
mod winit_input;
#[cfg(feature = "input-winit")]
pub use winit_input::winit_keycode_to_keycode;
#[cfg(feature = "input-winit")]
pub use winit_input::winit_mousebutton_to_mousebtn;
