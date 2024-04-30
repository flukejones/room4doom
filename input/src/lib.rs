//! All input handling. The output is generally a `TicCmd` used to run
//! inputs in the `Game` in a generalised way.
//!
//! Also does config options for controls.

pub mod config;

use std::collections::hash_set::HashSet;

use crate::config::InputConfig;
use gameplay::{tic_cmd::*, WeaponType};
use sdl2::{event::Event, keyboard::Scancode as Sc, mouse::MouseButton as Mb, EventPump};

#[derive(Default, Clone)]
pub struct InputEvents {
    key_state: HashSet<Sc>,
    mouse_state: HashSet<Mb>,
    mouse_delta: (i32, i32),
    mouse_scale: (i32, i32),
    turn_held: u32,
}
impl InputEvents {
    fn new(mouse_scale: (i32, i32)) -> Self {
        let mut i = Self::default();
        i.set_mouse_scale(mouse_scale);
        i
    }

    pub fn is_kb_pressed(&self, s: Sc) -> bool {
        self.key_state.contains(&s)
    }

    pub fn keys_pressed(&self) -> &HashSet<Sc> {
        &self.key_state
    }

    pub fn is_mb_pressed(&self, m: Mb) -> bool {
        self.mouse_state.contains(&m)
    }

    fn set_kb(&mut self, b: Sc) {
        self.key_state.insert(b);
    }

    fn unset_kb(&mut self, b: Sc) {
        self.key_state.remove(&b);
    }

    fn set_mb(&mut self, b: Mb) {
        self.mouse_state.insert(b);
    }

    fn unset_mb(&mut self, b: Mb) {
        self.mouse_state.remove(&b);
    }

    pub fn set_mouse_scale(&mut self, scale: (i32, i32)) {
        self.mouse_scale = scale;
    }

    fn reset_mouse_delta(&mut self) {
        self.mouse_delta = (0, 0);
    }

    fn set_mouse_pos(&mut self, state: (i32, i32)) {
        self.mouse_delta = (state.0 * self.mouse_scale.0, state.1 * self.mouse_scale.1);
    }

    pub fn build_tic_cmd(&mut self, cfg: &InputConfig) -> TicCmd {
        let mut cmd = TicCmd::default();

        // cmd->consistancy = consistancy[consoleplayer][maketic % BACKUPTICS];

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
            if let Some(key) = Sc::from_i32(30 + i as i32) {
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

        forward += self.mouse_delta.1;
        if strafe {
            side += mousex * 2;
        } else {
            cmd.angleturn -= (mousex * 0x8) as i16;
        }

        forward = forward.clamp(-MAXPLMOVE, MAXPLMOVE);
        side = side.clamp(-MAXPLMOVE, MAXPLMOVE);

        cmd.forwardmove += forward as i8;
        cmd.sidemove += side as i8;

        // TODO: special buttons
        // if (sendpause)
        // {
        //     sendpause = false;
        //     cmd->buttons = BT_SPECIAL | BTS_PAUSE;
        // }

        // if (sendsave)
        // {
        //     sendsave = false;
        //     cmd->buttons = BT_SPECIAL | BTS_SAVEGAME | (savegameslot << BTS_SAVESHIFT);
        // }
        self.reset_mouse_delta();

        cmd
    }
}

/// Fetch all input
pub struct Input {
    pump: EventPump,
    pub events: InputEvents,
    pub config: InputConfig,
    quit: bool,
}

impl Input {
    pub fn new(mut pump: EventPump) -> Input {
        pump.pump_events();
        Input {
            pump,
            events: InputEvents::new((10, 0)),
            config: InputConfig::default(),
            quit: false,
        }
    }

    /// The way this is set up to work is that for each `game tick`, a fresh set of events is
    /// gathered and stored. This results in a constant stream of events as long as an input is
    /// active/pressed. The event is released only once the key is up.
    ///
    /// `key_once_callback` is a provision to allow for functions where you don't want a continuous
    /// fast stream of "key pressed" by calling only on key-down event via SDL. This callback can
    /// return a bool - typically to signify that an event was taken.
    ///
    /// The results of the `update` are valid until the next `update` whereupon they are refreshed.
    ///
    /// **rust-sdl2** provides an `event_iter()`, but this isn't very useful unless we perform
    /// all the required actions in the same block that it is called in. It has the potential
    /// to cause delays in proccessing
    ///
    pub fn update(&mut self, mut key_once_callback: impl FnMut(Sc) -> bool) -> bool {
        let mut cb_res = false;
        while let Some(event) = self.pump.poll_event() {
            match event {
                Event::KeyDown {
                    scancode: Some(sc), ..
                } => {
                    if key_once_callback(sc) {
                        self.events.unset_kb(sc);
                        cb_res = true;
                    } else {
                        self.events.set_kb(sc);
                    }
                }
                Event::KeyUp {
                    scancode: Some(sc), ..
                } => {
                    self.events.unset_kb(sc);
                }
                Event::MouseButtonDown { mouse_btn, .. } => {
                    self.events.set_mb(mouse_btn);
                }
                Event::MouseButtonUp { mouse_btn, .. } => {
                    self.events.unset_mb(mouse_btn);
                }

                Event::MouseMotion {
                    x: _,
                    y: _,
                    xrel,
                    yrel,
                    ..
                } => {
                    self.events.set_mouse_pos((xrel, yrel));
                }

                Event::Quit { .. } => self.quit = true, // Early out if Quit
                _ => {}
            }
        }
        cb_res
    }
    pub fn get_quit(&self) -> bool {
        self.quit
    }
}
