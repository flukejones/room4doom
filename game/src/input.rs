use std::collections::hash_set::HashSet;

use doom_lib::{log::debug, tic_cmd::*, Cheats, Game, GameMission, PlayerCheat, WeaponType};
use sdl2::{
    event::Event,
    keyboard::{Keycode, Scancode as Sc},
    mouse::MouseButton as Mb,
    EventPump,
};

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

        for i in 0..WeaponType::NUMWEAPONS as u8 {
            if let Some(key) = Sc::from_i32('1' as i32 + 1) {
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
    pub fn update(&mut self, game: &mut Game) {
        while let Some(event) = self.pump.poll_event() {
            match event {
                Event::KeyDown {
                    scancode: Some(sc), ..
                } => {
                    self.cheat_check(sc, game);
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

    /// Cheats skip the ticcmd system and directly affect a game
    fn cheat_check(&mut self, sc: Sc, game: &mut Game) {
        let key = if let Some(key) = Keycode::from_scancode(sc) {
            key as u8 as char
        } else {
            return;
        };

        // TODO: need to check if netgame
        if !game.is_netgame() && !(game.game_skill() == doom_lib::Skill::Nightmare) {
            if self.cheats.god.check(key) {
                debug!("GODMODE");
                let player = &mut game.players[game.consoleplayer];
                player.cheats ^= PlayerCheat::Godmode as u32;
            } else if self.cheats.ammonokey.check(key) {
                debug!("IDFA");
                let player = &mut game.players[game.consoleplayer];
                player.armorpoints = 200;
                player.armortype = 2;

                for w in player.weaponowned.iter_mut() {
                    *w = true;
                }
                for (i, a) in player.ammo.iter_mut().enumerate() {
                    *a = player.maxammo[i];
                }
            } else if self.cheats.ammo.check(key) {
                debug!("IDKFA");
                let player = &mut game.players[game.consoleplayer];
                player.armorpoints = 200;
                player.armortype = 2;

                for w in player.weaponowned.iter_mut() {
                    *w = true;
                }
                for (i, a) in player.ammo.iter_mut().enumerate() {
                    *a = player.maxammo[i];
                }
                for k in player.cards.iter_mut() {
                    *k = true;
                }
            } else if (game.game_mission() == GameMission::Doom && self.cheats.noclip.check(key))
                || (game.game_mission() != GameMission::Doom
                    && self.cheats.commercial_noclip.check(key))
            {
                debug!("NOCLIP");
                let player = &mut game.players[game.consoleplayer];
                player.cheats ^= PlayerCheat::Noclip as u32;
            }
        }
    }
}

pub struct InputConfig {
    key_right: Sc,
    key_left: Sc,

    key_up: Sc,
    key_down: Sc,
    key_strafeleft: Sc,
    key_straferight: Sc,
    key_fire: Sc,
    key_use: Sc,
    key_strafe: Sc,
    key_speed: Sc,

    mousebfire: Mb,
    mousebstrafe: Mb,
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
