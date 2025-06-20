//! # Chip Implementation
//!
//! This module contains the main OPL2/OPL3 chip emulator implementation,
//! which coordinates all the channels, operators, and global state to
//! provide a complete FM synthesis system.

use crate::channel::synth_2percussion;
use crate::*;

impl Chip {
    /// Creates a new OPL chip instance
    ///
    /// # Arguments
    /// * `use_opl3` - Whether to enable OPL3 mode (stereo, 18 channels) or OPL2
    ///   mode (mono, 9 channels)
    ///
    /// # Returns
    /// A new `Chip` instance initialized for the specified mode
    pub fn new(use_opl3: bool) -> Self {
        init_tables();
        Self {
            opl3_active: if use_opl3 { -1 } else { 0 },
            ..Default::default()
        }
    }

    /// Advances the noise generator and returns current noise value
    ///
    /// The noise generator is used for percussion instruments and
    /// certain sound effects that require pseudo-random modulation.
    ///
    /// # Returns
    /// The current 32-bit noise value
    pub fn forward_noise(&mut self) -> u32 {
        self.noise_counter += self.noise_add;
        let mut count = self.noise_counter >> LFO_SH;
        self.noise_counter &= WAVE_MASK;

        while count > 0 {
            self.noise_value ^= (0x800302) & (0u32.wrapping_sub(self.noise_value & 1));
            self.noise_value >>= 1;
            count -= 1;
        }
        self.noise_value
    }

    /// Advances the LFO (Low Frequency Oscillator) and updates modulation
    /// values
    ///
    /// The LFO provides vibrato (frequency modulation) and tremolo (amplitude
    /// modulation) effects that can be applied to individual operators.
    ///
    /// # Arguments
    /// * `samples` - Number of samples to advance the LFO
    ///
    /// # Returns
    /// The actual number of samples the LFO was advanced
    pub fn forward_lfo(&mut self, samples: u32) -> u32 {
        // Use safe access to vibrato table
        let vibrato_val = VIBRATO_TABLE[(self.vibrato_index as usize >> 2) % VIBRATO_TABLE.len()];
        self.vibrato_sign = vibrato_val >> 7;
        self.vibrato_shift = (vibrato_val & 7) as u8 + self.vibrato_strength;

        // Update tremolo value from initialized table
        if let Some(tremolo_table) = TREMOLO_TABLE_DATA.get() {
            self.tremolo_value =
                tremolo_table[self.tremolo_index as usize] >> self.tremolo_strength;
        }

        let todo = LFO_MAX - self.lfo_counter;
        let mut count = (todo + self.lfo_add - 1) / self.lfo_add;

        if count > samples {
            count = samples;
            self.lfo_counter += count * self.lfo_add;
        } else {
            self.lfo_counter += count * self.lfo_add;
            self.lfo_counter &= LFO_MAX - 1;
            self.vibrato_index = (self.vibrato_index + 1) & 31;
            if (self.tremolo_index as usize + 1) < TREMOLO_TABLE {
                self.tremolo_index += 1;
            } else {
                self.tremolo_index = 0;
            }
        }
        count
    }

    /// Handles register 0xBD writes (rhythm mode control)
    ///
    /// Register 0xBD controls the rhythm/percussion mode and individual
    /// percussion instrument triggers. When rhythm mode is enabled,
    /// channels 6-8 are used for percussion instruments.
    ///
    /// # Arguments
    /// * `val` - The value to write to register 0xBD
    pub fn write_bd(&mut self, val: u8) {
        let change = self.reg_bd ^ val;
        if change == 0 {
            return;
        }

        self.reg_bd = val;

        // Update rhythm mode channels if rhythm mode changed
        if (change & 0x20) != 0 {
            if (val & 0x20) != 0 {
                // Enable rhythm mode
                self.chan[6].synth_handler = synth_2percussion;
                self.chan[7].synth_handler = synth_2percussion;
                self.chan[8].synth_handler = synth_2percussion;
            } else {
                // Disable rhythm mode - restore normal synthesis
                self.chan[6].reset_c0(self.opl3_active, self.reg_bd);
                self.chan[7].reset_c0(self.opl3_active, self.reg_bd);
                self.chan[8].reset_c0(self.opl3_active, self.reg_bd);
            }
        }

        // Handle individual percussion instrument triggers
        if (val & 0x20) != 0 {
            // Bass drum (channel 6)
            if (change & 0x10) != 0 {
                if (val & 0x10) != 0 {
                    self.chan[6].op[0].key_on(0x2);
                    self.chan[6].op[1].key_on(0x2);
                } else {
                    self.chan[6].op[0].key_off(0x2);
                    self.chan[6].op[1].key_off(0x2);
                }
            }

            // Snare drum (channel 7)
            if (change & 0x08) != 0 {
                if (val & 0x08) != 0 {
                    self.chan[7].op[1].key_on(0x2);
                } else {
                    self.chan[7].op[1].key_off(0x2);
                }
            }

            // Tom-tom (channel 8)
            if (change & 0x04) != 0 {
                if (val & 0x04) != 0 {
                    self.chan[8].op[0].key_on(0x2);
                } else {
                    self.chan[8].op[0].key_off(0x2);
                }
            }

            // Cymbal (channel 8)
            if (change & 0x02) != 0 {
                if (val & 0x02) != 0 {
                    self.chan[8].op[1].key_on(0x2);
                } else {
                    self.chan[8].op[1].key_off(0x2);
                }
            }

            // Hi-hat (channel 7)
            if (change & 0x01) != 0 {
                if (val & 0x01) != 0 {
                    self.chan[7].op[0].key_on(0x2);
                } else {
                    self.chan[7].op[0].key_off(0x2);
                }
            }
        }
    }

    /// Writes a value to an OPL register
    ///
    /// This is the main interface for configuring the OPL chip. Different
    /// register ranges control different aspects of the synthesis:
    /// - 0x20-0x35: Operator characteristics
    /// - 0x40-0x55: Operator volume and key scaling
    /// - 0x60-0x75: Operator envelope rates
    /// - 0x80-0x95: Operator sustain and release
    /// - 0xA0-0xA8: Channel frequency low byte
    /// - 0xB0-0xB8: Channel frequency high byte and key on/off
    /// - 0xC0-0xC8: Channel feedback and connection
    /// - 0xE0-0xF5: Operator waveform selection
    ///
    /// # Arguments
    /// * `reg` - The register address to write to
    /// * `val` - The value to write
    pub fn write_reg(&mut self, reg: u32, val: u8) {
        match (reg & 0xF0) >> 4 {
            0x00 => {
                if reg == 0x01 {
                    // Waveform select enable
                    self.wave_form_mask = if (val & 0x20) != 0 { 0x7 } else { 0x0 };
                } else if reg == 0x104 {
                    // OPL3 4-operator connection control
                    if ((self.reg104 ^ val) & 0x3F) == 0 {
                        return;
                    }
                    self.reg104 = 0x80 | (val & 0x3F);
                } else if reg == 0x105 {
                    // OPL3 mode enable
                    if ((self.opl3_active as u8 ^ val) & 1) == 0 {
                        return;
                    }
                    self.opl3_active = if (val & 1) != 0 { -1 } else { 0 };
                    // Reset all channel configurations when switching modes
                    for i in 0..18 {
                        let reg_c0 = self.chan[i].reg_c0;
                        self.chan[i].reg_c0 ^= 0xFF;
                        let opl3_active = self.opl3_active;
                        let reg_bd = self.reg_bd;
                        self.chan[i].write_c0_direct(reg_c0, opl3_active, reg_bd);
                    }
                } else if reg == 0x08 {
                    // Composite sine wave mode
                    self.reg08 = val;
                }
            }
            0x01 => {
                // Test register - not implemented
            }
            0x02 | 0x03 => {
                // Operator registers 0x20-0x35: characteristics
                let index = (((reg >> 3) & 0x20) | (reg & 0x1F)) as usize;
                let op_index = self.get_operator_index(index);
                if let Some((ch_idx, op_idx)) = op_index {
                    if ch_idx < 18 && op_idx < 2 {
                        self.chan[ch_idx].op[op_idx].write_20_direct(
                            val,
                            &self.freq_mul,
                            &self.attack_rates,
                            &self.linear_rates,
                        );
                    }
                }
            }
            0x04 | 0x05 => {
                // Operator registers 0x40-0x55: key scale level and total level
                let index = (((reg >> 3) & 0x20) | (reg & 0x1F)) as usize;
                let op_index = self.get_operator_index(index);
                if let Some((ch_idx, op_idx)) = op_index {
                    if ch_idx < 18 && op_idx < 2 {
                        self.chan[ch_idx].op[op_idx].write_40_direct(val);
                    }
                }
            }
            0x06 | 0x07 => {
                // Operator registers 0x60-0x75: attack rate and decay rate
                let index = (((reg >> 3) & 0x20) | (reg & 0x1F)) as usize;
                let op_index = self.get_operator_index(index);
                if let Some((ch_idx, op_idx)) = op_index {
                    if ch_idx < 18 && op_idx < 2 {
                        self.chan[ch_idx].op[op_idx].write_60_direct(
                            val,
                            &self.linear_rates,
                            &self.attack_rates,
                        );
                    }
                }
            }
            0x08 | 0x09 => {
                // Operator registers 0x80-0x95: sustain level and release rate
                let index = (((reg >> 3) & 0x20) | (reg & 0x1F)) as usize;
                let op_index = self.get_operator_index(index);
                if let Some((ch_idx, op_idx)) = op_index {
                    if ch_idx < 18 && op_idx < 2 {
                        self.chan[ch_idx].op[op_idx].write_80_direct(val, &self.linear_rates);
                    }
                }
            }
            0x0A => {
                // Channel registers 0xA0-0xA8: frequency low byte
                let index = (((reg >> 4) & 0x10) | (reg & 0xF)) as usize;
                let ch_index = self.get_channel_index(index);
                if ch_index < 18 {
                    self.chan[ch_index].write_a0_direct(
                        val,
                        self.reg104,
                        self.opl3_active as u8,
                        self.reg08,
                    );
                }
            }
            0x0B => {
                if reg == 0xBD {
                    // Rhythm mode control
                    self.write_bd(val);
                } else {
                    // Channel registers 0xB0-0xB8: frequency high byte and key on/off
                    let index = (((reg >> 4) & 0x10) | (reg & 0xF)) as usize;
                    let ch_index = self.get_channel_index(index);
                    if ch_index < 18 {
                        self.chan[ch_index].write_b0_direct(
                            val,
                            self.reg104,
                            self.opl3_active as u8,
                            self.reg08,
                        );
                    }
                }
            }
            0x0C => {
                // Channel registers 0xC0-0xC8: feedback and connection
                let index = (((reg >> 4) & 0x10) | (reg & 0xF)) as usize;
                let ch_index = self.get_channel_index(index);
                if ch_index < 18 {
                    self.chan[ch_index].write_c0_direct(val, self.opl3_active, self.reg_bd);
                }
            }
            0x0D => {
                // Unused register range
            }
            0x0E | 0x0F => {
                // Operator registers 0xE0-0xF5: waveform select
                let index = (((reg >> 3) & 0x20) | (reg & 0x1F)) as usize;
                let op_index = self.get_operator_index(index);
                if let Some((ch_idx, op_idx)) = op_index {
                    if ch_idx < 18 && op_idx < 2 {
                        self.chan[ch_idx].op[op_idx].write_e0_direct(
                            val,
                            self.wave_form_mask,
                            self.opl3_active as u8,
                        );
                    }
                }
            }
            _ => {
                // Unhandled register range
            }
        }
    }

    /// Writes an address to the OPL chip address port
    ///
    /// This simulates the two-step process of programming OPL chips:
    /// first write the register address, then write the data.
    /// In this implementation, we bypass the address latching and
    /// directly call write_reg.
    ///
    /// # Arguments
    /// * `port` - The port being written to (for OPL3 dual-chip support)
    /// * `val` - The register address
    ///
    /// # Returns
    /// The register address for use in subsequent data writes
    pub fn write_addr(&mut self, port: u32, val: u8) -> u32 {
        match port & 3 {
            0 => val as u32,
            2 => {
                // OPL3 second chip (registers 0x100-0x1FF)
                val as u32 + 0x100
            }
            _ => val as u32,
        }
    }

    /// Generates a block of mono audio samples
    ///
    /// This is the main audio generation method for OPL2 mode.
    /// It processes all active channels and combines their outputs
    /// to produce the final mono audio stream.
    ///
    /// # Arguments
    /// * `total` - Total number of samples to generate
    /// * `output` - Output buffer to write samples to
    pub fn generate_block_2(&mut self, total: usize, output: &mut [i32]) {
        // Clear output buffer
        output[..total].fill(0);

        let mut output_ptr = 0;
        let mut total = total;

        while total > 0 {
            let samples = total.min(512);

            // Update LFO for this block
            self.forward_lfo(samples as u32);

            // Process all channels using unsafe pointer manipulation to avoid borrow
            // checker
            let mut ch_idx = 0;
            while ch_idx < 9 {
                let chip_ptr = self as *mut Chip;
                let chan_ptr = unsafe { &mut (*chip_ptr).chan[ch_idx] };
                let handler = chan_ptr.synth_handler;
                handler(
                    chan_ptr,
                    unsafe { &mut *chip_ptr },
                    samples as u32,
                    &mut output[output_ptr..],
                );
                ch_idx += 1;
            }

            total -= samples;
            output_ptr += samples;
        }
    }

    /// Generates a block of stereo audio samples
    ///
    /// This is the main audio generation method for OPL3 mode.
    /// It processes all 18 channels and produces stereo output
    /// with proper left/right channel routing.
    ///
    /// # Arguments
    /// * `total` - Total number of sample pairs to generate
    /// * `output` - Stereo output buffer to write samples to
    pub fn generate_block_3(&mut self, total: usize, output: &mut [i32]) {
        // Clear output buffer (stereo, so double the size)
        output[..total * 2].fill(0);

        let mut output_ptr = 0;
        let mut total = total;

        while total > 0 {
            let samples = total.min(512);

            // Update LFO for this block
            self.forward_lfo(samples as u32);

            // Process all channels using unsafe pointer manipulation to avoid borrow
            // checker
            let mut ch_idx = 0;
            while ch_idx < 18 {
                let chip_ptr = self as *mut Chip;
                let chan_ptr = unsafe { &mut (*chip_ptr).chan[ch_idx] };
                let handler = chan_ptr.synth_handler;
                handler(
                    chan_ptr,
                    unsafe { &mut *chip_ptr },
                    samples as u32,
                    &mut output[output_ptr..],
                );
                ch_idx += 1;
            }

            total -= samples;
            output_ptr += samples * 2;
        }
    }

    /// Maps register index to operator location
    ///
    /// Converts the register index used in OPL register addresses
    /// to actual channel and operator indices for safe array access.
    ///
    /// # Arguments
    /// * `table_index` - The register-based index
    ///
    /// # Returns
    /// An optional tuple of (channel_index, operator_index)
    fn get_operator_index(&self, table_index: usize) -> Option<(usize, usize)> {
        // OPL2/3 operator mapping: convert register index to (channel, operator)
        match table_index {
            0x00..=0x15 => {
                let ch = table_index % 9;
                let op = if table_index < 9 { 0 } else { 1 };
                if ch < 9 { Some((ch, op)) } else { None }
            }
            0x20..=0x35 => {
                let adjusted = table_index - 0x20;
                let ch = (adjusted % 9) + 9;
                let op = if adjusted < 9 { 0 } else { 1 };
                if ch < 18 { Some((ch, op)) } else { None }
            }
            _ => None,
        }
    }

    /// Maps register index to channel location
    ///
    /// Converts the register index used in OPL register addresses
    /// to actual channel indices for safe array access.
    ///
    /// # Arguments
    /// * `table_index` - The register-based index
    ///
    /// # Returns
    /// The channel index (0-17 for OPL3, 0-8 for OPL2)
    fn get_channel_index(&self, table_index: usize) -> usize {
        // OPL2/3 channel mapping: convert register index to channel
        match table_index {
            0x00..=0x08 => table_index,
            0x10..=0x18 => table_index - 0x10 + 9,
            _ => 0,
        }
    }

    /// Sets up the chip for audio generation at the specified sample rate
    ///
    /// This method initializes all the internal rate tables, configures
    /// channel settings, and prepares the chip for audio synthesis.
    ///
    /// # Arguments
    /// * `rate` - The target sample rate in Hz (typically 44100 or 48000)
    pub fn setup(&mut self, rate: u32) {
        // Set up noise generator
        self.noise_add = (0.5 + OPLRATE * (1 << LFO_SH) as f64 / rate as f64) as u32;
        self.noise_counter = 0;
        self.noise_value = 1;

        // Set up LFO
        self.lfo_add = (0.5 + OPLRATE * (1 << LFO_SH) as f64 / rate as f64) as u32;
        self.lfo_counter = 0;
        self.vibrato_index = 0;
        self.tremolo_index = 0;

        // Initialize frequency multiplier table
        for i in 0..16 {
            let mul = FREQ_CREATE_TABLE[i] as f64;
            if i == 0 {
                self.freq_mul[i] = (0.5 + mul * (1 << WAVE_SH) as f64 / OPLRATE) as u32;
            } else {
                self.freq_mul[i] = (0.5 + mul * (1 << WAVE_SH) as f64 / OPLRATE) as u32;
            }
        }

        // Initialize attack rate table
        for i in 0..76 {
            let index = i % 4;
            let shift = (i / 4) as i32;
            let attack_samples = ATTACK_SAMPLES_TABLE[index] as f64;

            if shift < 62 {
                let samples = (attack_samples / (1i64 << shift) as f64) * rate as f64 / OPLRATE;
                self.attack_rates[i] = (0.5 + (1 << RATE_SH) as f64 / samples.max(1.0)) as u32;
            } else {
                self.attack_rates[i] = 1 << RATE_SH;
            }
        }

        // Initialize linear rate table (for decay and release)
        for i in 0..76 {
            let index = i % 4;
            let shift = (i / 4) as i32;
            let increase = ENVELOPE_INCREASE_TABLE[index] as f64;

            if shift < 60 {
                let samples = increase * (1i64 << shift) as f64 * rate as f64 / OPLRATE;
                self.linear_rates[i] = (0.5 + (1 << RATE_SH) as f64 / samples.max(1.0)) as u32;
            } else {
                self.linear_rates[i] = 1 << RATE_SH;
            }
        }

        // Set up global modulation
        self.vibrato_strength = 1;
        self.tremolo_strength = 2;

        // Initialize all channels
        for i in 0..18 {
            self.chan[i] = Channel::default();

            // Set up channel masks for 4-operator mode
            self.chan[i].four_mask = if i < 9 { 0x00 } else { 0x80 };

            // Set up specific 4-operator channel masks
            if i < 6 {
                let mask_val = if i < 3 { 1 << i } else { 1 << (i + 2) };
                if i % 3 == 0 {
                    self.chan[i].four_mask |= mask_val;
                    self.chan[i + 3].four_mask |= 0x80 | mask_val;
                }
            }
        }

        // Set up 4-operator channel linking for OPL3
        self.chan[0].four_mask = 0x00 | (1 << 0);
        self.chan[1].four_mask = 0x80 | (1 << 0);
        self.chan[2].four_mask = 0x00 | (1 << 1);
        self.chan[3].four_mask = 0x80 | (1 << 1);
        self.chan[4].four_mask = 0x00 | (1 << 2);
        self.chan[5].four_mask = 0x80 | (1 << 2);
        self.chan[9].four_mask = 0x00 | (1 << 3);
        self.chan[10].four_mask = 0x80 | (1 << 3);
        self.chan[11].four_mask = 0x00 | (1 << 4);
        self.chan[12].four_mask = 0x80 | (1 << 4);
        self.chan[13].four_mask = 0x00 | (1 << 5);
        self.chan[14].four_mask = 0x80 | (1 << 5);

        // Mark percussion channels
        self.chan[6].four_mask = 0x40;
        self.chan[7].four_mask = 0x40;
        self.chan[8].four_mask = 0x40;

        // Offset tables are no longer used - we use direct indexing for safety

        // Clear everything in OPL3 mode
        self.write_reg(0x105, 0x1);
        self.write_reg(0x104, 0x0);
        self.write_reg(0x105, 0x0);
        self.write_reg(0x104, 0x0);

        // Initialize all registers to default values
        for i in 0..512 {
            self.write_reg(i, 0);
        }

        // Set up initial configuration
        self.reg_bd = 0;
        self.reg04 = 0;
        self.reg08 = 0;
        self.reg104 = 0;
        self.opl3_active = 0;
        self.wave_form_mask = 0;
    }
}
