//! Game cheats. These are what players type in, e.g, `iddqd`

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
            mus: Cheat::new("idmus", 0),
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

    pub fn intercept() {}
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

    pub fn get_parameter(&self) -> String {
        String::from_iter(self.parameter_buf[0..self.parameter_chars].iter())
    }
}