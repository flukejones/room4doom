//! # Channel Implementation
//!
//! This module contains the implementation of OPL2/OPL3 channels, which combine
//! two operators to create a single FM voice. Channels can operate in different
//! synthesis modes and handle various connection types between operators.

use crate::*;

impl Channel {
    /// Creates a new channel with default settings
    ///
    /// # Returns
    /// A new `Channel` instance initialized to default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a mutable reference to one of the channel's operators
    ///
    /// # Arguments
    /// * `index` - The operator index (0 or 1), automatically masked to valid range
    ///
    /// # Returns
    /// A mutable reference to the requested operator
    pub fn op(&mut self, index: usize) -> &mut Operator {
        &mut self.op[index & 1]
    }

    /// Returns a mutable reference to an operator (simplified implementation)
    ///
    /// This is a simplified version that doesn't perform cross-channel access
    /// for safety reasons. In the original implementation, this would access
    /// operators from other channels for 4-operator mode.
    ///
    /// # Arguments
    /// * `index` - The operator index
    ///
    /// # Returns
    /// A mutable reference to a local operator
    pub fn op_from_channel(&mut self, index: usize) -> &mut Operator {
        &mut self.op[index & 1]
    }

    /// Updates channel data and refreshes operator rates if needed
    ///
    /// This method updates the channel's frequency and key scaling data,
    /// and triggers rate updates on both operators if the key code changed.
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing rate tables
    /// * `data` - New channel data value
    pub fn set_chan_data(&mut self, chip: &Chip, data: u32) {
        self.set_chan_data_direct(data);

        let change = self.chan_data ^ data;
        if (change & (0xff << SHIFT_KEYCODE)) != 0 {
            self.op[0].update_rates(chip);
            self.op[1].update_rates(chip);
        }
    }

    /// Directly updates channel data and refreshes operator parameters
    ///
    /// This is the low-level method that actually updates the channel data
    /// and triggers frequency and attenuation updates on both operators.
    ///
    /// # Arguments
    /// * `data` - New channel data value containing frequency and key scaling info
    pub fn set_chan_data_direct(&mut self, data: u32) {
        let change = self.chan_data ^ data;
        self.chan_data = data;
        self.op[0].chan_data = data;
        self.op[1].chan_data = data;

        // Update frequency calculations for both operators
        self.op[0].update_frequency();
        self.op[1].update_frequency();

        // Update attenuation if KSL base changed
        if (change & (0xff << SHIFT_KSLBASE)) != 0 {
            self.op[0].update_attenuation();
            self.op[1].update_attenuation();
        }
    }

    /// Updates the channel frequency using chip settings
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing register values
    /// * `four_op` - Four-operator mode flags (unused in safe implementation)
    pub fn update_frequency(&mut self, chip: &Chip, four_op: u8) {
        self.update_frequency_direct(four_op, chip.reg08);
    }

    /// Directly updates the channel frequency with specified parameters
    ///
    /// Calculates the key code and KSL base values from the current frequency
    /// data and updates the channel accordingly.
    ///
    /// # Arguments
    /// * `four_op` - Four-operator mode flags (simplified in safe implementation)
    /// * `reg08` - Register 0x08 value for composite sine wave mode
    pub fn update_frequency_direct(&mut self, four_op: u8, reg08: u8) {
        let mut data = self.chan_data & 0xffff;

        // Look up KSL base value from the initialized table
        let ksl_base = if let Some(ksl_table) = KSL_TABLE.get() {
            ksl_table[(data >> 6) as usize] as u32
        } else {
            0
        };

        let mut key_code = (data & 0x1c00) >> 9;

        // Adjust key code based on composite sine wave mode
        if (reg08 & 0x40) != 0 {
            key_code |= (data & 0x100) >> 8;
        } else {
            key_code |= (data & 0x200) >> 9;
        }

        // Combine frequency data with key code and KSL base
        data |= (key_code << SHIFT_KEYCODE) | (ksl_base << SHIFT_KSLBASE);
        self.set_chan_data_direct(data);

        // Note: Four-operator handling removed for safety
        // In the original implementation, this would update linked channels
        if (four_op & 0x3f) != 0 {
            // TODO: Implement safe multi-channel access if needed
        }
    }

    /// Handles register 0xA0-0xA8 writes (frequency low byte)
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing register values
    /// * `val` - The value to write to the register
    pub fn write_a0(&mut self, chip: &Chip, val: u8) {
        self.write_a0_direct(val, chip.reg104, chip.opl3_active as u8, chip.reg08);
    }

    /// Directly processes frequency low byte register writes
    ///
    /// # Arguments
    /// * `val` - The frequency low byte value
    /// * `reg104` - Register 0x104 value (OPL3 4-op connections)
    /// * `opl3_active` - OPL3 mode flag
    /// * `reg08` - Register 0x08 value
    pub fn write_a0_direct(&mut self, val: u8, reg104: u8, opl3_active: u8, reg08: u8) {
        let four_op = reg104 & opl3_active & self.four_mask;
        if four_op > 0x80 {
            return;
        }

        let change = (self.chan_data ^ val as u32) & 0xff;
        if change != 0 {
            self.chan_data ^= change;
            self.update_frequency_direct(four_op, reg08);
        }
    }

    /// Handles register 0xB0-0xB8 writes (frequency high byte + key on/off)
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing register values
    /// * `val` - The value to write to the register
    pub fn write_b0(&mut self, chip: &Chip, val: u8) {
        self.write_b0_direct(val, chip.reg104, chip.opl3_active as u8, chip.reg08);
    }

    /// Directly processes frequency high byte and key on/off register writes
    ///
    /// This method handles both frequency updates and key on/off events.
    /// When a key is pressed (bit 5 set), both operators are activated.
    /// When released, both operators enter their release phase.
    ///
    /// # Arguments
    /// * `val` - The register value containing frequency high bits and key state
    /// * `reg104` - Register 0x104 value (OPL3 4-op connections)
    /// * `opl3_active` - OPL3 mode flag
    /// * `reg08` - Register 0x08 value
    pub fn write_b0_direct(&mut self, val: u8, reg104: u8, opl3_active: u8, reg08: u8) {
        let four_op = reg104 & opl3_active & self.four_mask;
        if four_op > 0x80 {
            return;
        }

        // Update frequency high bits if they changed
        let change = (self.chan_data ^ ((val as u32) << 8)) & 0x1f00;
        if change != 0 {
            self.chan_data ^= change;
            self.update_frequency_direct(four_op, reg08);
        }

        // Handle key on/off events (bit 5)
        if ((val ^ self.reg_b0) & 0x20) == 0 {
            return; // No key state change
        }

        self.reg_b0 = val;
        if (val & 0x20) != 0 {
            // Key on: activate both operators
            self.op[0].key_on(0x1);
            self.op[1].key_on(0x1);
            if (four_op & 0x3f) != 0 {
                // TODO: Implement safe multi-channel key_on for 4-op mode
            }
        } else {
            // Key off: release both operators
            self.op[0].key_off(0x1);
            self.op[1].key_off(0x1);
            if (four_op & 0x3f) != 0 {
                // TODO: Implement safe multi-channel key_off for 4-op mode
            }
        }
    }

    /// Handles register 0xC0-0xC8 writes (feedback and connection)
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing register values
    /// * `val` - The value to write to the register
    pub fn write_c0(&mut self, chip: &Chip, val: u8) {
        self.write_c0_direct(val, chip.opl3_active, chip.reg_bd);
    }

    /// Directly processes feedback and connection register writes
    ///
    /// This method configures how the two operators are connected:
    /// - Bit 0: Connection type (0=FM, 1=AM)
    /// - Bits 1-3: Feedback amount
    /// - Bits 4-5: Output routing (OPL3 only)
    ///
    /// # Arguments
    /// * `val` - The register value
    /// * `opl3_active` - OPL3 mode flag
    /// * `reg_bd` - Register 0xBD value (rhythm mode)
    pub fn write_c0_direct(&mut self, val: u8, opl3_active: i8, reg_bd: u8) {
        let change = val ^ self.reg_c0;
        if change == 0 {
            return;
        }

        self.reg_c0 = val;

        // Configure feedback amount (bits 1-3)
        self.feedback = (val >> 1) & 7;
        if self.feedback != 0 {
            self.feedback = 9 - self.feedback; // Convert to shift amount
        } else {
            self.feedback = 31; // No feedback
        }

        // Set synthesis mode based on connection type and OPL3 mode
        if opl3_active != 0 {
            // OPL3 mode
            if (self.four_mask & 0x40) != 0 && (reg_bd & 0x20) != 0 {
                // Percussion channel - would need special handling
            } else if (val & 1) != 0 {
                self.synth_handler = synth_3am; // Additive mode
            } else {
                self.synth_handler = synth_3fm; // FM mode
            }

            // Configure stereo output routing (OPL3 only)
            self.mask_left = if (val & 0x10) != 0 { -1 } else { 0 };
            self.mask_right = if (val & 0x20) != 0 { -1 } else { 0 };
        } else {
            // OPL2 mode
            if (self.four_mask & 0x40) != 0 && (reg_bd & 0x20) != 0 {
                // Percussion channel - would need special handling
            } else if (val & 1) != 0 {
                self.synth_handler = synth_2am; // Additive mode
            } else {
                self.synth_handler = synth_2fm; // FM mode
            }
        }
    }

    /// Resets the C0 register configuration
    ///
    /// This forces a reconfiguration of the channel's synthesis mode
    /// by temporarily changing the register value and then writing it back.
    ///
    /// # Arguments
    /// * `chip` - Reference to the chip for accessing register values
    pub fn reset_c0(&mut self, opl3_active: i8, reg_bd: u8) {
        let val = self.reg_c0;
        self.reg_c0 ^= 0xff; // Force change detection
        self.write_c0_direct(val, opl3_active, reg_bd);
    }

    /// Generates percussion instrument sounds
    ///
    /// This method implements the special percussion mode where channel 6-8
    /// are used to generate bass drum, snare drum, tom-tom, hi-hat, and cymbal.
    /// The implementation is simplified for safety reasons.
    ///
    /// # Arguments
    /// * `chip` - Mutable reference to the chip for noise generation
    /// * `output` - Output buffer to write samples to
    /// * `opl3_mode` - Whether OPL3 stereo mode is active
    pub fn generate_percussion(&mut self, chip: &mut Chip, output: &mut [i32], opl3_mode: bool) {
        // Generate the base sound using FM synthesis
        let mod_val = ((self.old[0] + self.old[1]) as u32) >> self.feedback;
        self.old[0] = self.old[1];
        self.old[1] = self.op[0].get_sample(mod_val as i32);

        let mod_val = if (self.reg_c0 & 1) != 0 {
            0 // Additive mode
        } else {
            self.old[0] // FM mode
        };
        let mut sample = self.op[1].get_sample(mod_val);

        // Generate noise and phase information for percussion instruments
        let noise_bit = chip.forward_noise() & 0x1;
        let c2 = self.op[0].forward_wave();
        let c5 = self.op[1].forward_wave();
        let phase_bit = if (((c2 & 0x88) ^ ((c2 << 5) & 0x80)) | ((c5 ^ (c5 << 2)) & 0x20)) != 0 {
            0x02
        } else {
            0x00
        };

        // Hi-Hat (simplified using local operators)
        let hh_vol = self.op[0].forward_volume();
        if !env_silent(hh_vol as i32) {
            let hh_index = (phase_bit << 8) | (0x34 << (phase_bit ^ (noise_bit << 1)));
            sample += self.op[0].get_wave(hh_index, hh_vol);
        }

        // Snare Drum (simplified using local operators)
        let sd_vol = self.op[1].forward_volume();
        if !env_silent(sd_vol as i32) {
            let sd_index = (0x100 + (c2 & 0x100)) ^ (noise_bit << 8);
            sample += self.op[1].get_wave(sd_index, sd_vol);
        }

        // Apply percussion amplification
        sample <<= 1;

        // Write to output buffer
        if opl3_mode {
            output[0] += sample;
            output[1] += sample;
        } else {
            output[0] += sample;
        }
    }

    /// Template method for generating audio samples in different synthesis modes
    ///
    /// This is the main synthesis engine that handles all the different
    /// operator connection modes and generates the final audio output.
    ///
    /// # Arguments
    /// * `chip` - Mutable reference to the chip for LFO and noise
    /// * `samples` - Number of samples to generate
    /// * `output` - Output buffer to write samples to
    /// * `mode` - The synthesis mode to use
    ///
    /// # Returns
    /// The number of channels processed (1 for mono, 2 for stereo)
    pub fn block_template(
        &mut self,
        chip: &mut Chip,
        samples: u32,
        output: &mut [i32],
        mode: SynthMode,
    ) -> usize {
        // Early silence detection to optimize performance
        match mode {
            SynthMode::Sm2AM | SynthMode::Sm3AM => {
                if self.op[0].silent() && self.op[1].silent() {
                    self.old[0] = 0;
                    self.old[1] = 0;
                    return 1;
                }
            }
            SynthMode::Sm2FM | SynthMode::Sm3FM => {
                if self.op[1].silent() {
                    self.old[0] = 0;
                    self.old[1] = 0;
                    return 1;
                }
            }
            SynthMode::Sm3FMFM => {
                if self.op[1].silent() {
                    self.old[0] = 0;
                    self.old[1] = 0;
                    return 2;
                }
            }
            SynthMode::Sm3AMFM => {
                if self.op[0].silent() && self.op[1].silent() {
                    self.old[0] = 0;
                    self.old[1] = 0;
                    return 2;
                }
            }
            SynthMode::Sm3FMAM => {
                if self.op[1].silent() && self.op[0].silent() {
                    self.old[0] = 0;
                    self.old[1] = 0;
                    return 2;
                }
            }
            SynthMode::Sm3AMAM => {
                if self.op[0].silent() && self.op[1].silent() {
                    self.old[0] = 0;
                    self.old[1] = 0;
                    return 2;
                }
            }
            _ => {}
        }

        // Prepare operators with current LFO and tremolo values
        self.op[0].prepare(chip);
        self.op[1].prepare(chip);

        // Generate the requested number of samples
        for i in 0..samples as usize {
            match mode {
                SynthMode::Sm2Percussion => {
                    self.generate_percussion(chip, &mut output[i..], false);
                    continue;
                }
                SynthMode::Sm3Percussion => {
                    self.generate_percussion(chip, &mut output[i * 2..], true);
                    continue;
                }
                _ => {}
            }

            // Calculate feedback modulation
            let mod_val = ((self.old[0] + self.old[1]) as u32) >> self.feedback;
            self.old[0] = self.old[1];
            self.old[1] = self.op[0].get_sample(mod_val as i32);

            let out0 = self.old[0];

            // Generate sample based on synthesis mode
            let sample = match mode {
                SynthMode::Sm2AM | SynthMode::Sm3AM => {
                    // Additive synthesis: add both operators
                    out0 + self.op[1].get_sample(0)
                }
                SynthMode::Sm2FM | SynthMode::Sm3FM => {
                    // FM synthesis: operator 1 modulates operator 2
                    self.op[1].get_sample(out0)
                }
                SynthMode::Sm3FMFM => {
                    // 3-op FM chain (simplified for safety)
                    let next = self.op[1].get_sample(out0);
                    self.op[0].get_sample(next)
                }
                SynthMode::Sm3AMFM => {
                    // 3-op mixed mode (simplified for safety)
                    let sample = out0;
                    let next = self.op[1].get_sample(0);
                    sample + self.op[0].get_sample(next)
                }
                SynthMode::Sm3FMAM => {
                    // 3-op mixed mode (simplified for safety)
                    let sample = self.op[1].get_sample(out0);
                    let next = self.op[0].get_sample(0);
                    sample + next
                }
                SynthMode::Sm3AMAM => {
                    // 3-op additive mode (simplified for safety)
                    let sample = out0;
                    let next = self.op[1].get_sample(0);
                    sample + next + self.op[0].get_sample(0)
                }
                _ => 0,
            };

            // Write sample to output buffer with appropriate routing
            match mode {
                SynthMode::Sm2AM | SynthMode::Sm2FM => {
                    // Mono output
                    output[i] += sample;
                }
                SynthMode::Sm3AM
                | SynthMode::Sm3FM
                | SynthMode::Sm3FMFM
                | SynthMode::Sm3AMFM
                | SynthMode::Sm3FMAM
                | SynthMode::Sm3AMAM => {
                    // Stereo output with panning masks
                    output[i * 2] += sample & (self.mask_left as i32);
                    output[i * 2 + 1] += sample & (self.mask_right as i32);
                }
                _ => {}
            }
        }

        // Return number of channels processed
        match mode {
            SynthMode::Sm2AM | SynthMode::Sm2FM | SynthMode::Sm3AM | SynthMode::Sm3FM => 1,
            SynthMode::Sm3FMFM | SynthMode::Sm3AMFM | SynthMode::Sm3FMAM | SynthMode::Sm3AMAM => 2,
            SynthMode::Sm2Percussion | SynthMode::Sm3Percussion => 3,
            _ => 1,
        }
    }
}

// Synthesis handler function implementations
// These functions provide the interface between the chip's synthesis
// dispatcher and the channel's template method.

/// 2-operator FM synthesis handler
pub fn synth_2fm(ch: &mut Channel, chip: &mut Chip, samples: u32, output: &mut [i32]) -> usize {
    ch.block_template(chip, samples, output, SynthMode::Sm2FM)
}

/// 2-operator AM synthesis handler
pub fn synth_2am(ch: &mut Channel, chip: &mut Chip, samples: u32, output: &mut [i32]) -> usize {
    ch.block_template(chip, samples, output, SynthMode::Sm2AM)
}

/// 3-operator FM synthesis handler (OPL3)
pub fn synth_3fm(ch: &mut Channel, chip: &mut Chip, samples: u32, output: &mut [i32]) -> usize {
    ch.block_template(chip, samples, output, SynthMode::Sm3FM)
}

/// 3-operator AM synthesis handler (OPL3)
pub fn synth_3am(ch: &mut Channel, chip: &mut Chip, samples: u32, output: &mut [i32]) -> usize {
    ch.block_template(chip, samples, output, SynthMode::Sm3AM)
}

/// 3-operator FM-FM synthesis handler (OPL3)
pub fn synth_3fmfm(ch: &mut Channel, chip: &mut Chip, samples: u32, output: &mut [i32]) -> usize {
    ch.block_template(chip, samples, output, SynthMode::Sm3FMFM)
}

/// 3-operator AM-FM synthesis handler (OPL3)
pub fn synth_3amfm(ch: &mut Channel, chip: &mut Chip, samples: u32, output: &mut [i32]) -> usize {
    ch.block_template(chip, samples, output, SynthMode::Sm3AMFM)
}

/// 3-operator FM-AM synthesis handler (OPL3)
pub fn synth_3fmam(ch: &mut Channel, chip: &mut Chip, samples: u32, output: &mut [i32]) -> usize {
    ch.block_template(chip, samples, output, SynthMode::Sm3FMAM)
}

/// 3-operator AM-AM synthesis handler (OPL3)
pub fn synth_3amam(ch: &mut Channel, chip: &mut Chip, samples: u32, output: &mut [i32]) -> usize {
    ch.block_template(chip, samples, output, SynthMode::Sm3AMAM)
}

/// 2-operator percussion synthesis handler
pub fn synth_2percussion(
    ch: &mut Channel,
    chip: &mut Chip,
    samples: u32,
    output: &mut [i32],
) -> usize {
    ch.block_template(chip, samples, output, SynthMode::Sm2Percussion)
}

/// 3-operator percussion synthesis handler (OPL3)
pub fn synth_3percussion(
    ch: &mut Channel,
    chip: &mut Chip,
    samples: u32,
    output: &mut [i32],
) -> usize {
    ch.block_template(chip, samples, output, SynthMode::Sm3Percussion)
}
