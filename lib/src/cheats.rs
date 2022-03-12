pub struct Cheats {
    pub god: Cheat,
    pub mus: Cheat,
    pub ammo: Cheat,
    pub ammonokey: Cheat,
    pub noclip: Cheat,
    pub commercial_noclip: Cheat,
    pub powerup: [Cheat; 7],
    pub choppers: Cheat,
    pub clev: Cheat,
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
