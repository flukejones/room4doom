use std::collections::hash_set::HashSet;

use sdl2::event::Event;
use sdl2::keyboard::Scancode as Sc;
use sdl2::mouse::MouseButton as Mb;
use sdl2::EventPump;

use crate::{doom_def::WeaponType, tic_cmd::*};

#[derive(Debug, Default, Clone)]
pub struct InputEvents {
    key_state:   HashSet<Sc>,
    mouse_state: HashSet<Mb>,
    mouse_pos:   (i32, i32),
    turn_held:   u32,
}
impl InputEvents {
    pub fn clear(&mut self) {
        self.key_state.clear();
        self.mouse_state.clear();
        self.mouse_pos = (0, 0)
    }

    pub fn is_kb_pressed(&self, s: Sc) -> bool { self.key_state.contains(&s) }

    pub fn is_mb_pressed(&self, m: Mb) -> bool { self.mouse_state.contains(&m) }

    pub fn mouse_pos(&self) -> (i32, i32) { self.mouse_pos }

    fn set_kb(&mut self, b: Sc) { self.key_state.insert(b); }

    fn unset_kb(&mut self, b: Sc) { self.key_state.remove(&b); }

    fn set_mb(&mut self, b: Mb) { self.mouse_state.insert(b); }

    fn unset_mb(&mut self, b: Mb) { self.mouse_state.remove(&b); }

    pub fn set_mouse_pos(&mut self, state: (i32, i32)) {
        self.mouse_pos = state;
    }

    pub fn build_tic_cmd(&mut self, cfg: &InputConfig) -> TicCmd {
        let mut cmd = TicCmd::default();

        // cmd->consistancy = consistancy[consoleplayer][maketic % BACKUPTICS];

        let strafe = self.is_kb_pressed(cfg.key_strafe)
            || self.is_mb_pressed(cfg.mousebstrafe);
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

        let turn_speed;
        if self.turn_held < 6 {
            turn_speed = 2;
        } else {
            turn_speed = speed;
        }

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

        if self.is_kb_pressed(cfg.key_fire) {
            cmd.buttons |= TIC_CMD_BUTTONS.bt_attack;
        }

        if self.is_kb_pressed(cfg.key_use) {
            cmd.buttons |= TIC_CMD_BUTTONS.bt_use;
        }

        for i in 0..WeaponType::NUMWEAPONS as u8 {
            if let Some(key) = Sc::from_i32('1' as i32 + 1 as i32) {
                if self.is_kb_pressed(key) {
                    cmd.buttons |= TIC_CMD_BUTTONS.bt_change;
                    cmd.buttons |= i << TIC_CMD_BUTTONS.bt_weaponshift;
                }
            }
        }

        // Mouse
        if self.is_mb_pressed(cfg.mousebforward) {
            forward += FORWARDMOVE[speed];
        }

        let mousex = self.mouse_pos.0;

        forward += self.mouse_pos.1;
        if strafe {
            side += mousex * 2;
        } else {
            cmd.angleturn -= (mousex * 0x8) as i16;
        }

        if forward > MAXPLMOVE {
            forward = MAXPLMOVE;
        } else if forward < -MAXPLMOVE {
            forward = -MAXPLMOVE;
        }
        if side > MAXPLMOVE {
            side = MAXPLMOVE;
        } else if side < -MAXPLMOVE {
            side = -MAXPLMOVE;
        }

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

        cmd
    }
}

/// Fetch all input
pub struct Input {
    pump:           EventPump,
    pub tic_events: InputEvents,
    pub config:     InputConfig,
    quit:           bool,
}

impl Input {
    pub fn new(mut pump: EventPump) -> Input {
        pump.pump_events();
        Input {
            pump,
            tic_events: InputEvents::default(),
            config: InputConfig::default(),
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
                Event::KeyDown { scancode, .. } => {
                    if let Some(sc) = scancode {
                        self.tic_events.set_kb(sc);
                    }
                }
                Event::KeyUp { scancode, .. } => {
                    if let Some(sc) = scancode {
                        self.tic_events.unset_kb(sc);
                    }
                }
                Event::MouseButtonDown { mouse_btn, .. } => {
                    self.tic_events.set_mb(mouse_btn);
                }
                Event::MouseButtonUp { mouse_btn, .. } => {
                    self.tic_events.unset_mb(mouse_btn);
                }

                Event::MouseMotion { x, y, .. } => {
                    self.tic_events.set_mouse_pos((x, y));
                }

                Event::Quit { .. } => self.quit = true, // Early out if Quit
                _ => {}
            }
        }
    }
    pub fn get_quit(&self) -> bool { self.quit }
}

pub struct InputConfig {
    key_right: Sc,
    key_left:  Sc,

    key_up:          Sc,
    key_down:        Sc,
    key_strafeleft:  Sc,
    key_straferight: Sc,
    key_fire:        Sc,
    key_use:         Sc,
    key_strafe:      Sc,
    key_speed:       Sc,

    mousebfire:    Mb,
    mousebstrafe:  Mb,
    mousebforward: Mb,
}

impl Default for InputConfig {
    fn default() -> Self {
        InputConfig {
            key_right: Sc::Right,
            key_left:  Sc::Left,

            key_up:          Sc::Up,
            key_down:        Sc::Down,
            key_strafeleft:  Sc::Comma,
            key_straferight: Sc::Period,
            key_fire:        Sc::RCtrl,
            key_use:         Sc::Space,
            key_strafe:      Sc::RAlt,
            key_speed:       Sc::RShift,

            mousebfire:    Mb::Left,
            mousebstrafe:  Mb::Middle,
            mousebforward: Mb::Right,
        }
    }
}

impl InputConfig {
    pub fn key_right(&self) -> Sc { self.key_right }

    pub fn key_left(&self) -> Sc { self.key_left }

    pub fn key_up(&self) -> Sc { self.key_up }

    pub fn key_down(&self) -> Sc { self.key_down }

    pub fn key_strafeleft(&self) -> Sc { self.key_strafeleft }

    pub fn key_straferight(&self) -> Sc { self.key_straferight }

    pub fn key_fire(&self) -> Sc { self.key_fire }

    pub fn key_use(&self) -> Sc { self.key_use }

    pub fn key_strafe(&self) -> Sc { self.key_strafe }

    pub fn key_speed(&self) -> Sc { self.key_speed }

    pub fn mousebfire(&self) -> Mb { self.mousebfire }

    pub fn mousebstrafe(&self) -> Mb { self.mousebstrafe }

    pub fn mousebforward(&self) -> Mb { self.mousebforward }
}
