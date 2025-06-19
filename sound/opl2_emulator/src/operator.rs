//! # Operator Implementation
//!
//! This module contains the implementation of OPL2/OPL3 operators, which are
//! the fundamental building blocks of FM synthesis. Each operator generates
//! a sine wave with controllable frequency, amplitude, and envelope, and can
//! either be used as a carrier (audible output) or modulator (frequency modulation).

use crate::*;

impl Operator {
    /// Creates a new operator with default settings
    ///
    /// # Returns
    /// A new `Operator` instance initialized to default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the operator's envelope state and corresponding volume handler
    ///
    /// The volume handler function determines how the envelope progresses
    /// through its different phases (attack, decay, sustain, release).
    ///
    /// # Arguments
    /// * `state` - The new envelope state to set
    pub fn set_state(&mut self, state: OperatorState) {
        self.state = state;
        self.vol_handler = match state {
            OperatorState::Off => volume_off,
            OperatorState::Release => volume_release,
            OperatorState::Sustain => volume_sustain,
            OperatorState::Decay => volume_decay,
            OperatorState::Attack => volume_attack,
        };
    }

    /// Updates the attack rate using chip's rate tables
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing attack rate tables
    pub fn update_attack(&mut self, chip: &Chip) {
        self.update_attack_direct(&chip.attack_rates);
    }

    /// Directly updates the attack rate with provided rate table
    ///
    /// Calculates the attack increment based on the attack rate setting
    /// in register 0x60 (bits 4-7) and the current key scale rate.
    ///
    /// # Arguments
    /// * `attack_rates` - Reference to the attack rate lookup table
    pub fn update_attack_direct(&mut self, attack_rates: &[u32; 76]) {
        let rate = self.reg60 >> 4;
        if rate != 0 {
            let val = (rate << 2) + self.ksr;
            self.attack_add = attack_rates[val as usize];
            self.rate_zero &= !(1 << 4);
        } else {
            self.attack_add = 0;
            self.rate_zero |= 1 << 4;
        }
    }

    /// Updates the decay rate using chip's rate tables
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing linear rate tables
    pub fn update_decay(&mut self, chip: &Chip) {
        self.update_decay_direct(&chip.linear_rates);
    }

    /// Directly updates the decay rate with provided rate table
    ///
    /// Calculates the decay increment based on the decay rate setting
    /// in register 0x60 (bits 0-3) and the current key scale rate.
    ///
    /// # Arguments
    /// * `linear_rates` - Reference to the linear rate lookup table
    pub fn update_decay_direct(&mut self, linear_rates: &[u32; 76]) {
        let rate = self.reg60 & 0xf;
        if rate != 0 {
            let val = (rate << 2) + self.ksr;
            self.decay_add = linear_rates[val as usize];
            self.rate_zero &= !(1 << 3);
        } else {
            self.decay_add = 0;
            self.rate_zero |= 1 << 3;
        }
    }

    /// Updates the release rate using chip's rate tables
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing linear rate tables
    pub fn update_release(&mut self, chip: &Chip) {
        self.update_release_direct(&chip.linear_rates);
    }

    /// Directly updates the release rate with provided rate table
    ///
    /// Calculates the release increment based on the release rate setting
    /// in register 0x80 (bits 0-3) and the current key scale rate.
    /// Also handles sustain mode configuration.
    ///
    /// # Arguments
    /// * `linear_rates` - Reference to the linear rate lookup table
    pub fn update_release_direct(&mut self, linear_rates: &[u32; 76]) {
        let rate = self.reg80 & 0xf;
        if rate != 0 {
            let val = (rate << 2) + self.ksr;
            self.release_add = linear_rates[val as usize];
            self.rate_zero &= !(1 << 1);
            if (self.reg20 & MASK_SUSTAIN) == 0 {
                self.rate_zero &= !(1 << 2);
            }
        } else {
            self.rate_zero |= 1 << 1;
            self.release_add = 0;
            if (self.reg20 & MASK_SUSTAIN) == 0 {
                self.rate_zero |= 1 << 2;
            }
        }
    }

    /// Updates the operator's attenuation based on total level and key scaling
    ///
    /// Combines the programmed total level with key scale level (KSL) to
    /// determine the final attenuation. Higher notes can be automatically
    /// attenuated based on the KSL setting to simulate acoustic instruments.
    pub fn update_attenuation(&mut self) {
        let ksl_base = ((self.chan_data >> SHIFT_KSLBASE) & 0xff) as u8;
        let tl = (self.reg40 & 0x3f) as u32;
        let ksl_shift = KSL_SHIFT_TABLE[(self.reg40 >> 6) as usize];

        self.total_level = (tl << (ENV_BITS - 7)) as i32;
        self.total_level += (((ksl_base as u32) << ENV_EXTRA) >> ksl_shift) as i32;
    }

    /// Updates the operator's frequency calculations
    ///
    /// Recalculates wave increment and vibrato settings based on the
    /// current channel frequency data and frequency multiplier setting.
    pub fn update_frequency(&mut self) {
        let freq = self.chan_data & ((1 << 10) - 1);
        let block = (self.chan_data >> 10) & 0xff;

        #[cfg(feature = "wave_precision")]
        {
            let block = 7 - block;
            self.wave_add = (freq * self.freq_mul) >> block;
        }
        #[cfg(not(feature = "wave_precision"))]
        {
            self.wave_add = (freq << block) * self.freq_mul;
        }

        // Configure vibrato if enabled
        if (self.reg20 & MASK_VIBRATO) != 0 {
            self.vib_strength = (freq >> 7) as u8;

            #[cfg(feature = "wave_precision")]
            {
                self.vibrato = (self.vib_strength as u32 * self.freq_mul) >> block;
            }
            #[cfg(not(feature = "wave_precision"))]
            {
                self.vibrato = (self.vib_strength as u32) << block * self.freq_mul;
            }
        } else {
            self.vib_strength = 0;
            self.vibrato = 0;
        }
    }

    /// Updates all envelope and frequency rates using chip tables
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing rate tables
    pub fn update_rates(&mut self, chip: &Chip) {
        self.update_rates_direct(&chip.attack_rates, &chip.linear_rates);
    }

    /// Directly updates all rates with provided rate tables
    ///
    /// Recalculates attack, decay, and release rates when the key scale rate
    /// (KSR) value changes. KSR makes envelopes faster for higher notes.
    ///
    /// # Arguments
    /// * `attack_rates` - Reference to the attack rate lookup table
    /// * `linear_rates` - Reference to the linear rate lookup table
    pub fn update_rates_direct(&mut self, attack_rates: &[u32; 76], linear_rates: &[u32; 76]) {
        let new_ksr = ((self.chan_data >> SHIFT_KEYCODE) & 0xff) as u8;
        let new_ksr = if (self.reg20 & MASK_KSR) == 0 {
            new_ksr >> 2
        } else {
            new_ksr
        };

        if self.ksr == new_ksr {
            return;
        }

        self.ksr = new_ksr;
        self.update_attack_direct(attack_rates);
        self.update_decay_direct(linear_rates);
        self.update_release_direct(linear_rates);
    }

    /// Advances the rate counter and returns the increment amount
    ///
    /// This is used internally by the envelope generator to determine
    /// how much to advance the envelope in the current sample.
    ///
    /// # Arguments
    /// * `add` - The rate increment value
    ///
    /// # Returns
    /// The amount to advance the envelope
    pub fn rate_forward(&mut self, add: u32) -> i32 {
        self.rate_index += add;
        let ret = (self.rate_index >> RATE_SH) as i32;
        self.rate_index &= RATE_MASK;
        ret
    }

    /// Template method for envelope volume calculation
    ///
    /// Handles the envelope progression for a specific state, updating
    /// the volume and potentially transitioning to the next state.
    ///
    /// # Arguments
    /// * `state` - The envelope state to process
    ///
    /// # Returns
    /// The current envelope volume level
    pub fn template_volume(&mut self, state: OperatorState) -> i32 {
        let mut vol = self.volume;

        match state {
            OperatorState::Off => return ENV_MAX,
            OperatorState::Attack => {
                let change = self.rate_forward(self.attack_add);
                if change == 0 {
                    return vol;
                }
                // Exponential attack curve
                vol += ((!(vol as u32) as i32) * change) >> 3;
                if vol < ENV_MIN {
                    self.volume = ENV_MIN;
                    self.rate_index = 0;
                    self.set_state(OperatorState::Decay);
                    return ENV_MIN;
                }
            }
            OperatorState::Decay => {
                vol += self.rate_forward(self.decay_add);
                if vol >= self.sustain_level {
                    if vol >= ENV_MAX {
                        self.volume = ENV_MAX;
                        self.set_state(OperatorState::Off);
                        return ENV_MAX;
                    }
                    self.rate_index = 0;
                    self.set_state(OperatorState::Sustain);
                }
            }
            OperatorState::Sustain => {
                // Check if sustain is disabled (EG type bit)
                if (self.reg20 & MASK_SUSTAIN) != 0 {
                    return vol;
                }
                // Continue to release if sustain is disabled
                vol += self.rate_forward(self.release_add);
                if vol >= ENV_MAX {
                    self.volume = ENV_MAX;
                    self.set_state(OperatorState::Off);
                    return ENV_MAX;
                }
            }
            OperatorState::Release => {
                vol += self.rate_forward(self.release_add);
                if vol >= ENV_MAX {
                    self.volume = ENV_MAX;
                    self.set_state(OperatorState::Off);
                    return ENV_MAX;
                }
            }
        }

        self.volume = vol;
        vol
    }

    /// Calculates the current volume including envelope and tremolo
    ///
    /// # Returns
    /// The final volume level ready for waveform lookup
    pub fn forward_volume(&mut self) -> u32 {
        (self.current_level as i32 + (self.vol_handler)(self)) as u32
    }

    /// Advances the wave position and returns the current wave index
    ///
    /// # Returns
    /// The current position in the waveform table
    pub fn forward_wave(&mut self) -> u32 {
        self.wave_index += self.wave_current;
        self.wave_index >> WAVE_SH
    }

    /// Handles register 0x20-0x35 writes (operator characteristics)
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing rate tables
    /// * `val` - The value to write to the register
    pub fn write_20(&mut self, chip: &Chip, val: u8) {
        self.write_20_direct(val, &chip.freq_mul, &chip.attack_rates, &chip.linear_rates);
    }

    /// Directly processes operator characteristic register writes
    ///
    /// Register 0x20 controls:
    /// - Bit 0-3: Frequency multiplier
    /// - Bit 4: Key scale rate (KSR)
    /// - Bit 5: Sustain mode
    /// - Bit 6: Vibrato enable
    /// - Bit 7: Tremolo enable
    ///
    /// # Arguments
    /// * `val` - The register value
    /// * `freq_mul` - Frequency multiplier lookup table
    /// * `attack_rates` - Attack rate lookup table
    /// * `linear_rates` - Linear rate lookup table
    pub fn write_20_direct(
        &mut self,
        val: u8,
        freq_mul: &[u32; 16],
        attack_rates: &[u32; 76],
        linear_rates: &[u32; 76],
    ) {
        let change = self.reg20 ^ val;
        if change == 0 {
            return;
        }

        self.reg20 = val;

        // Configure tremolo mask
        self.tremolo_mask = (val as i8) >> 7;
        self.tremolo_mask &= !((1 << ENV_EXTRA) - 1) as i8;

        // Update rates if KSR setting changed
        if (change & MASK_KSR) != 0 {
            self.update_rates_direct(attack_rates, linear_rates);
        }

        // Configure sustain mode
        if (self.reg20 & MASK_SUSTAIN) != 0 || self.release_add == 0 {
            self.rate_zero |= 1 << 2;
        } else {
            self.rate_zero &= !(1 << 2);
        }

        // Update frequency multiplier and vibrato if changed
        if (change & (0xf | MASK_VIBRATO)) != 0 {
            self.freq_mul = freq_mul[(val & 0xf) as usize];
            self.update_frequency();
        }
    }

    /// Handles register 0x40-0x55 writes (key scale level and total level)
    ///
    /// # Arguments
    /// * `_chip` - Reference to the chip (unused in current implementation)
    /// * `val` - The value to write to the register
    pub fn write_40(&mut self, _chip: &Chip, val: u8) {
        self.write_40_direct(val);
    }

    /// Directly processes key scale level and total level register writes
    ///
    /// Register 0x40 controls:
    /// - Bit 0-5: Total level (volume)
    /// - Bit 6-7: Key scale level (KSL)
    ///
    /// # Arguments
    /// * `val` - The register value
    pub fn write_40_direct(&mut self, val: u8) {
        if (self.reg40 ^ val) == 0 {
            return;
        }
        self.reg40 = val;
        self.update_attenuation();
    }

    /// Handles register 0x60-0x75 writes (attack rate and decay rate)
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing rate tables
    /// * `val` - The value to write to the register
    pub fn write_60(&mut self, chip: &Chip, val: u8) {
        self.write_60_direct(val, &chip.linear_rates, &chip.attack_rates);
    }

    /// Directly processes attack and decay rate register writes
    ///
    /// Register 0x60 controls:
    /// - Bit 0-3: Decay rate
    /// - Bit 4-7: Attack rate
    ///
    /// # Arguments
    /// * `val` - The register value
    /// * `linear_rates` - Linear rate lookup table
    /// * `attack_rates` - Attack rate lookup table
    pub fn write_60_direct(&mut self, val: u8, linear_rates: &[u32; 76], attack_rates: &[u32; 76]) {
        let change = self.reg60 ^ val;
        self.reg60 = val;

        if (change & 0x0f) != 0 {
            self.update_decay_direct(linear_rates);
        }
        if (change & 0xf0) != 0 {
            self.update_attack_direct(attack_rates);
        }
    }

    /// Handles register 0x80-0x95 writes (sustain level and release rate)
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing rate tables
    /// * `val` - The value to write to the register
    pub fn write_80(&mut self, chip: &Chip, val: u8) {
        self.write_80_direct(val, &chip.linear_rates);
    }

    /// Directly processes sustain level and release rate register writes
    ///
    /// Register 0x80 controls:
    /// - Bit 0-3: Release rate
    /// - Bit 4-7: Sustain level
    ///
    /// # Arguments
    /// * `val` - The register value
    /// * `linear_rates` - Linear rate lookup table
    pub fn write_80_direct(&mut self, val: u8, linear_rates: &[u32; 76]) {
        let change = self.reg80 ^ val;
        if change == 0 {
            return;
        }

        self.reg80 = val;

        // Calculate sustain level
        let mut sustain = val >> 4;
        sustain |= (sustain + 1) & 0x10;
        self.sustain_level = ((sustain as u32) << (ENV_BITS - 5)) as i32;

        // Update release rate if changed
        if (change & 0x0f) != 0 {
            self.update_release_direct(linear_rates);
        }
    }

    /// Handles register 0xE0-0xF5 writes (waveform select)
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing waveform settings
    /// * `val` - The value to write to the register
    pub fn write_e0(&mut self, chip: &Chip, val: u8) {
        self.write_e0_direct(val, chip.wave_form_mask, chip.opl3_active as u8);
    }

    /// Directly processes waveform select register writes
    ///
    /// Register 0xE0 controls:
    /// - Bit 0-2: Waveform select (0=sine, 1=half-sine, 2=abs-sine, 3=pulse-sine)
    ///
    /// # Arguments
    /// * `val` - The register value
    /// * `wave_form_mask` - Mask for available waveforms
    /// * `opl3_active` - OPL3 mode flag for extended waveforms
    pub fn write_e0_direct(&mut self, val: u8, wave_form_mask: u8, opl3_active: u8) {
        if (self.reg_e0 ^ val) == 0 {
            return;
        }

        let wave_form = val & ((0x3 & wave_form_mask) | (0x7 & opl3_active));
        self.reg_e0 = val;

        // Set up waveform table pointers
        self.wave_base =
            (WAVE_BASE_TABLE[wave_form as usize] as usize) * std::mem::size_of::<i16>();
        self.wave_start = (WAVE_START_TABLE[wave_form as usize] as u32) << WAVE_SH;
        self.wave_mask = WAVE_MASK_TABLE[wave_form as usize] as u32;
    }

    /// Checks if the operator is currently silent
    ///
    /// An operator is considered silent if its total volume is below the
    /// silence threshold and its envelope is not actively changing.
    ///
    /// # Returns
    /// `true` if the operator is silent and can be optimized out
    pub fn silent(&self) -> bool {
        if !env_silent(self.total_level + self.volume) {
            return false;
        }
        if (self.rate_zero & (1 << (self.state as u8))) == 0 {
            return false;
        }
        true
    }

    /// Prepares the operator for sample generation
    ///
    /// Updates the current level with tremolo modulation and applies
    /// vibrato to the wave frequency if enabled.
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for LFO values
    pub fn prepare(&mut self, chip: &Chip) {
        // Apply tremolo (amplitude modulation)
        self.current_level =
            (self.total_level as u32) + ((chip.tremolo_value as u32) & (self.tremolo_mask as u32));

        // Set base wave frequency
        self.wave_current = self.wave_add;

        // Apply vibrato (frequency modulation) if enabled
        if (self.vib_strength >> chip.vibrato_shift) != 0 {
            let mut add = (self.vibrato >> chip.vibrato_shift) as i32;
            let neg = chip.vibrato_sign as i32;
            add = (add ^ neg) - neg;
            self.wave_current = (self.wave_current as i32 + add) as u32;
        }
    }

    /// Activates the operator (key on)
    ///
    /// Starts the envelope from the attack phase and resets wave position.
    /// Multiple key-on sources can be combined using the mask parameter.
    ///
    /// # Arguments
    /// * `mask` - Bitmask indicating the key-on source
    pub fn key_on(&mut self, mask: u8) {
        if self.key_on == 0 {
            // Start from beginning
            self.wave_index = self.wave_start;
            self.rate_index = 0;
            self.set_state(OperatorState::Attack);
        }
        self.key_on |= mask;
    }

    /// Deactivates the operator (key off)
    ///
    /// Removes the specified key-on source. If no sources remain active,
    /// the operator enters its release phase.
    ///
    /// # Arguments
    /// * `mask` - Bitmask indicating the key-on source to remove
    pub fn key_off(&mut self, mask: u8) {
        self.key_on &= !mask;
        if self.key_on == 0 {
            if self.state != OperatorState::Off {
                self.set_state(OperatorState::Release);
            }
        }
    }

    /// Looks up a waveform sample at the specified index and volume
    ///
    /// # Arguments
    /// * `index` - Position in the waveform table
    /// * `vol` - Volume level for attenuation lookup
    ///
    /// # Returns
    /// The computed waveform sample
    pub fn get_wave(&self, index: u32, vol: u32) -> i32 {
        let wave_index =
            (self.wave_base / std::mem::size_of::<i16>()) + ((index & self.wave_mask) as usize);

        // Use safe OnceLock access
        let wave = if wave_index < 8 * 512 {
            if let Some(wave_table) = WAVE_TABLE.get() {
                wave_table[wave_index] as i32
            } else {
                0
            }
        } else {
            0
        };

        let mul_index = (vol >> ENV_EXTRA) as usize;
        let mul_value = if mul_index < 384 {
            if let Some(mul_table) = MUL_TABLE.get() {
                mul_table[mul_index] as i32
            } else {
                0
            }
        } else {
            0
        };

        (wave * mul_value) >> MUL_SH
    }

    /// Generates a single audio sample from this operator
    ///
    /// This is the main sample generation method that combines the
    /// waveform lookup with envelope and modulation processing.
    ///
    /// # Arguments
    /// * `modulation` - Frequency modulation input from other operators
    ///
    /// # Returns
    /// The generated audio sample
    pub fn get_sample(&mut self, modulation: i32) -> i32 {
        let vol = self.forward_volume();
        if env_silent(vol as i32) {
            // Silent optimization: advance wave position but return silence
            self.wave_index += self.wave_current;
            return 0;
        } else {
            // Generate normal sample
            let index = self.forward_wave();
            let index = ((index as i32) + modulation) as u32;
            return self.get_wave(index, vol);
        }
    }
}

// Volume handler function implementations
// These functions implement the different phases of the ADSR envelope

/// Volume handler for silent operators
pub fn volume_off(_op: &mut Operator) -> i32 {
    ENV_MAX
}

/// Volume handler for operators in release phase
pub fn volume_release(op: &mut Operator) -> i32 {
    op.template_volume(OperatorState::Release)
}

/// Volume handler for operators in sustain phase
pub fn volume_sustain(op: &mut Operator) -> i32 {
    op.template_volume(OperatorState::Sustain)
}

/// Volume handler for operators in decay phase
pub fn volume_decay(op: &mut Operator) -> i32 {
    op.template_volume(OperatorState::Decay)
}

/// Volume handler for operators in attack phase
pub fn volume_attack(op: &mut Operator) -> i32 {
    op.template_volume(OperatorState::Attack)
}
