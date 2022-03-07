pub struct Cheats {
    cheat_god: Cheat,
    cheat_mus: Cheat,
    cheat_ammo: Cheat,
    cheat_ammonokey: Cheat,
    cheat_noclip: Cheat,
    cheat_commercial_noclip: Cheat,

    cheat_powerup: [Cheat; 7],
    cheat_choppers: Cheat,
    cheat_clev: Cheat,
    cheat_mypos: Cheat,
}

impl Cheats {
    pub fn new() -> Self {
        Self {
            cheat_god: Cheat::new("iddqd", 2),
            cheat_mus: Cheat::new("idmus", 0),
            cheat_ammo: Cheat::new("idkfa", 0),
            cheat_ammonokey: Cheat::new("idfa", 0),
            cheat_noclip: Cheat::new("idspispopd", 0),
            cheat_commercial_noclip: Cheat::new("idclip", 0),
            cheat_powerup: [
                Cheat::new("idbeholdv", 0),
                Cheat::new("idbeholds", 0),
                Cheat::new("idbeholdi", 0),
                Cheat::new("idbeholdr", 0),
                Cheat::new("idbeholda", 0),
                Cheat::new("idbeholdl", 0),
                Cheat::new("idbehold", 0),
            ],
            cheat_choppers: Cheat::new("idchoppers", 0),
            cheat_clev: Cheat::new("idclev", 2),
            cheat_mypos: Cheat::new("idmypos", 0),
        }
    }

    pub fn intercept() {}
}

pub struct Cheat {
    /// The sequence of chars to accept
    sequence: &'static str,
    /// Total sequence length including parameters
    seq_len: usize,
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
            seq_len: 0,
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
