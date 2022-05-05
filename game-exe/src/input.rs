//! All input handling. The output is generally a `TicCmd` used to run
//! inputs in the `Game` in a generalised way.

use std::collections::hash_set::HashSet;

use gameplay::{tic_cmd::*, WeaponType};
use sdl2::{event::Event, keyboard::Scancode as Sc, mouse::MouseButton as Mb, EventPump};

use crate::{cheats::Cheats, game::Game};

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
                    cmd.buttons |= i << TIC_CMD_BUTTONS.bt_weaponshift;
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
        self.reset_mouse_delta();

        cmd
    }
}

/// Fetch all input
pub struct Input {
    pump: EventPump,
    pub tic_events: InputEvents,
    pub config: InputConfig,
    quit: bool,
    cheats: Cheats,
}

impl Input {
    pub fn new(mut pump: EventPump) -> Input {
        pump.pump_events();
        Input {
            pump,
            tic_events: InputEvents::new((10, 0)),
            config: InputConfig::default(),
            quit: false,
            cheats: Cheats::new(),
        }
    }

    /// The way this is set up to work is that for each `game-exe tick`, a fresh set of event is
    /// gathered and stored. Then for that single game-exe tick, every part of the game-exe can ask
    /// `Input` for results without the results being removed.
    ///
    /// The results of the `update` are valid until the next `update` whereupon they are refreshed.
    ///
    /// **rust-sdl2** provides an `event_iter()`, but this isn't very useful unless we perform
    /// all the required actions in the same block that it is called in. It has the potential
    /// to cause delays in proccessing
    ///
    pub fn update(&mut self, game: &mut Game) {
        while let Some(event) = self.pump.poll_event() {
            match event {
                Event::KeyDown {
                    scancode: Some(sc), ..
                } => {
                    self.cheats.check_input(sc, game);
                    self.tic_events.set_kb(sc);
                }
                Event::KeyUp {
                    scancode: Some(sc), ..
                } => {
                    self.tic_events.unset_kb(sc);
                }
                Event::MouseButtonDown { mouse_btn, .. } => {
                    self.tic_events.set_mb(mouse_btn);
                }
                Event::MouseButtonUp { mouse_btn, .. } => {
                    self.tic_events.unset_mb(mouse_btn);
                }

                Event::MouseMotion {
                    x: _,
                    y: _,
                    xrel,
                    yrel,
                    ..
                } => {
                    self.tic_events.set_mouse_pos((xrel, yrel));
                }

                Event::Quit { .. } => self.quit = true, // Early out if Quit
                _ => {}
            }
        }
    }
    pub fn get_quit(&self) -> bool {
        self.quit
    }
}

use serde::{de, Deserialize, Serialize, Serializer};

fn serialize_scancode<S>(sc: &Sc, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_i32(*sc as i32)
}

fn deserialize_scancode<'de, D>(deserializer: D) -> Result<Sc, D::Error>
where
    D: de::Deserializer<'de>,
{
    let sc: i32 = de::Deserialize::deserialize(deserializer)?;
    let sc = Sc::from_i32(sc).unwrap_or_else(|| panic!("Could not deserialise key config"));
    Ok(sc)
}

fn serialize_mb<S>(sc: &Mb, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_u8(*sc as u8)
}

fn deserialize_mb<'de, D>(deserializer: D) -> Result<Mb, D::Error>
where
    D: de::Deserializer<'de>,
{
    let sc: u8 = de::Deserialize::deserialize(deserializer)?;
    let sc = Mb::from_ll(sc);
    Ok(sc)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    key_right: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    key_left: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    key_up: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    key_down: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    key_strafeleft: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    key_straferight: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    key_fire: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    key_use: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    key_strafe: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    key_speed: Sc,

    #[serde(serialize_with = "serialize_mb")]
    #[serde(deserialize_with = "deserialize_mb")]
    mousebfire: Mb,
    #[serde(serialize_with = "serialize_mb")]
    #[serde(deserialize_with = "deserialize_mb")]
    mousebstrafe: Mb,
    #[serde(serialize_with = "serialize_mb")]
    #[serde(deserialize_with = "deserialize_mb")]
    mousebforward: Mb,
}

impl Default for InputConfig {
    fn default() -> Self {
        InputConfig {
            key_right: Sc::Right,
            key_left: Sc::Left,

            key_up: Sc::W,
            key_down: Sc::S,
            key_strafeleft: Sc::A,
            key_straferight: Sc::D,
            key_fire: Sc::RCtrl,
            key_use: Sc::Space,
            key_strafe: Sc::RAlt,
            key_speed: Sc::LShift,

            mousebfire: Mb::Left,
            mousebstrafe: Mb::Middle,
            mousebforward: Mb::Right,
        }
    }
}