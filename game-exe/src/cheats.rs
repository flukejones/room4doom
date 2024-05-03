//! Game cheats. These are what players type in, e.g, `iddqd`

use gameplay::log::debug;
use gameplay::{english, GameMission, PlayerCheat, Skill, WeaponType};
use gamestate::Game;
use gamestate_traits::sdl2::keyboard::{Keycode, Scancode};
use gamestate_traits::GameTraits;
use sound_traits::MusTrack;

pub struct Cheats {
    /// `iddqd`: Invulnerable to all (except massive end-of-level damage)
    pub god: Cheat,
    /// `idmus##`: Select music to play, ## is 01-nn
    pub mus: Cheat,
    /// `idkfa: Give all ammo and keys
    pub ammo: Cheat,
    /// `idfa`: Give only ammo
    pub ammonokey: Cheat,
    /// `idspispopd`: no-clip, Doom 1 version
    pub noclip: Cheat,
    /// `idclip`: no-clip, Doom 2 version
    pub commercial_noclip: Cheat,
    /// Give a powerup:
    /// - `idbeholdv`: Invulnerability
    /// - `idbeholds`: Go beserk
    /// - `idbeholdi`: Pertial invisibility
    /// - `idbeholdr`: Radiation suit
    /// - `idbeholda`: Area map
    /// - `idbeholdl`: Light amp visor
    pub powerup: [Cheat; 7],
    /// `idchoppers`: Chainsaw and invulnerability
    pub choppers: Cheat,
    /// `idclev##`: Change level, ## is E#M# or MAP## (01-32)
    pub clev: Cheat,
    /// `idmypos`: Coords and compass direction
    pub mypos: Cheat,
}

impl Cheats {
    pub fn new() -> Self {
        Self {
            god: Cheat::new("iddqd", 0),
            mus: Cheat::new("idmus", 2),
            ammo: Cheat::new("idkfa", 0),
            ammonokey: Cheat::new("idfa", 0),
            noclip: Cheat::new("idspispopd", 0),
            commercial_noclip: Cheat::new("idclip", 0),
            powerup: [
                Cheat::new("idbeholdv", 0),
                Cheat::new("idbeholds", 0),
                Cheat::new("idbeholdi", 0),
                Cheat::new("idbeholdr", 0),
                Cheat::new("idbeholda", 0),
                Cheat::new("idbeholdl", 0),
                Cheat::new("idbehold", 0),
            ],
            choppers: Cheat::new("idchoppers", 0),
            clev: Cheat::new("idclev", 2),
            mypos: Cheat::new("idmypos", 0),
        }
    }

    /// Cheats skip the ticcmd system and directly affect a game-exe
    pub fn check_input(&mut self, sc: Scancode, game: &mut Game) {
        let key = if let Some(key) = Keycode::from_scancode(sc) {
            key as u8 as char
        } else {
            return;
        };

        if !game.is_netgame() && !(game.game_skill() == Skill::Nightmare) {
            if self.god.check(key) {
                let player = &mut game.players[game.consoleplayer];
                player.status.cheats ^= PlayerCheat::Godmode as u32;

                if player.status.cheats & PlayerCheat::Godmode as u32 != 0 {
                    if let Some(mobj) = player.mobj_mut() {
                        mobj.health = 100;
                    }
                    player.status.health = 100;
                    player.message = Some(english::STSTR_DQDON);
                } else {
                    player.message = Some(english::STSTR_DQDOFF);
                }
            } else if self.ammonokey.check(key) {
                let player = &mut game.players[game.consoleplayer];
                player.status.armorpoints = 200;
                player.status.armortype = 2;

                for w in player.status.weaponowned.iter_mut() {
                    *w = true;
                }
                for (i, a) in player.status.ammo.iter_mut().enumerate() {
                    *a = player.status.maxammo[i];
                }
                player.message = Some(english::STSTR_FAADDED);
            } else if self.ammo.check(key) {
                let player = &mut game.players[game.consoleplayer];
                player.status.armorpoints = 200;
                player.status.armortype = 2;

                for w in player.status.weaponowned.iter_mut() {
                    *w = true;
                }
                for (i, a) in player.status.ammo.iter_mut().enumerate() {
                    *a = player.status.maxammo[i];
                }
                for k in player.status.cards.iter_mut() {
                    *k = true;
                }
                player.message = Some(english::STSTR_KFAADDED);
            } else if self.choppers.check(key) {
                let player = &mut game.players[game.consoleplayer];
                player.status.weaponowned[WeaponType::Chainsaw as usize] = true;
                player.pendingweapon = WeaponType::Chainsaw;
                player.status.cheats &= PlayerCheat::Godmode as u32;
                if let Some(mobj) = player.mobj_mut() {
                    mobj.health = 100;
                }
                player.status.health = 100;
                player.message = Some(english::STSTR_CHOPPERS);
            } else if (game.game_mission() == GameMission::Doom && self.noclip.check(key))
                || (game.game_mission() != GameMission::Doom && self.commercial_noclip.check(key))
            {
                let player = &mut game.players[game.consoleplayer];
                player.status.cheats ^= PlayerCheat::Noclip as u32;
                if player.status.cheats & PlayerCheat::Noclip as u32 != 0 {
                    player.message = Some(english::STSTR_NCON);
                } else {
                    player.message = Some(english::STSTR_NCOFF);
                }
            } else if self.mus.check(key) {
                debug!(
                    "MUS{}{}",
                    self.mus.parameter_buf[0], self.mus.parameter_buf[1]
                );
                let s = format!("{}{}", self.mus.parameter_buf[0], self.mus.parameter_buf[1]);
                if let Ok(s) = s.as_str().parse::<u8>() {
                    let s = MusTrack::from(s);
                    game.change_music(s);
                    game.players[game.consoleplayer].message = Some(english::STSTR_MUS);
                } else {
                    game.players[game.consoleplayer].message = Some(english::STSTR_NOMUS);
                }
            } else if self.mypos.check(key) {
                debug!("MYPOS",);
                let player = &mut game.players[game.consoleplayer];
                if let Some(mobj) = player.mobj() {
                    println!("MYPOS: X:{} Y:{}", mobj.xy.x as i32, mobj.xy.y as i32);
                }
            }
        }
    }
}

pub struct Cheat {
    /// The sequence of chars to accept
    sequence: &'static str,
    /// `char` read so far
    chars_read: usize,
    /// How many parameter chars there can be
    parameter_chars: usize,
    /// Parameter chars read so far
    parameter_chars_read: usize,
    /// Input buffer for parameters
    parameter_buf: [char; 5],
}

impl Cheat {
    pub const fn new(seq: &'static str, parameters: usize) -> Self {
        Self {
            sequence: seq,
            chars_read: 0,
            parameter_chars: parameters,
            parameter_chars_read: 0,
            parameter_buf: [' '; 5],
        }
    }

    /// Doom function name `cht_CheckCheat`
    pub fn check(&mut self, key: char) -> bool {
        if self.chars_read < self.sequence.len() {
            if key as u8 == self.sequence.as_bytes()[self.chars_read] {
                self.chars_read += 1;
            } else {
                self.chars_read = 0;
            }

            self.parameter_chars_read = 0;
        } else if self.parameter_chars_read < self.parameter_chars {
            self.parameter_buf[self.parameter_chars_read] = key;
            self.parameter_chars_read += 1;
        }

        if self.chars_read >= self.sequence.len()
            && self.parameter_chars_read >= self.parameter_chars
        {
            self.chars_read = 0;
            self.parameter_chars_read = 0;
            return true;
        }

        false
    }
}
