//! OPL2/OPL3 FM synthesis emulator
//!
//! Clean-room Rust implementation based on DOSBox DBOPL (WAVE_TABLEMUL mode).
//! Public API: `init_tables()`, `Chip::new()`, `Chip::setup()`,
//! `Chip::write_reg()`, `Chip::generate_block_2()`.

pub mod player;
pub use player::OplPlayerState;

use std::f64::consts::PI;
use std::sync::OnceLock;

// ── Core constants ──────────────────────────────────────────────────────────

const OPLRATE: f64 = 14318180.0 / 288.0;

const WAVE_BITS: u32 = 10;
const WAVE_SH: u32 = 32 - WAVE_BITS;
const WAVE_MASK: u32 = (1 << WAVE_SH) - 1;

const LFO_SH: u32 = WAVE_SH - 10;
const LFO_MAX: u32 = 256 << LFO_SH;

const ENV_BITS: u32 = 9;
const ENV_EXTRA: u32 = 0;
const ENV_MIN: i32 = 0;
const ENV_MAX: i32 = 511;
const ENV_LIMIT: i32 = 384;

const RATE_SH: u32 = 24;
const RATE_MASK: u32 = (1 << RATE_SH) - 1;
const MUL_SH: u32 = 16;

const SHIFT_KSLBASE: u32 = 16;
const SHIFT_KEYCODE: u32 = 24;

const MASK_KSR: u8 = 0x10;
const MASK_SUSTAIN: u8 = 0x20;
const MASK_VIBRATO: u8 = 0x40;

const TREMOLO_TABLE_LEN: usize = 52;

// ── Static lookup tables ────────────────────────────────────────────────────

const KSL_CREATE: [u8; 16] = [64, 32, 24, 19, 16, 12, 11, 10, 8, 6, 5, 4, 3, 2, 1, 0];
const FREQ_CREATE: [u8; 16] = [1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 20, 24, 24, 30, 30];
const ATTACK_SAMPLES: [u8; 13] = [69, 55, 46, 40, 35, 29, 23, 20, 19, 15, 11, 10, 9];
const ENV_INCREASE: [u8; 13] = [4, 5, 6, 7, 8, 10, 12, 14, 16, 20, 24, 28, 32];
const VIBRATO_TBL: [i8; 8] = [1, 0, 1, 30, -127, -128, -127, -98];
const KSL_SHIFT: [u8; 4] = [31, 1, 2, 0];
const WAVE_BASE_TBL: [u16; 8] = [0x000, 0x200, 0x200, 0x800, 0xA00, 0xC00, 0x100, 0x400];
const WAVE_MASK_TBL: [u16; 8] = [1023, 1023, 511, 511, 1023, 1023, 512, 1023];
const WAVE_START_TBL: [u16; 8] = [512, 0, 0, 0, 0, 512, 512, 256];

// ── Generated tables (initialized once) ─────────────────────────────────────

static MUL_TABLE: OnceLock<[u16; 384]> = OnceLock::new();
static WAVE_TABLE: OnceLock<[i16; 4096]> = OnceLock::new();
static KSL_TABLE: OnceLock<[u8; 128]> = OnceLock::new();
static TREMOLO_DATA: OnceLock<[u8; TREMOLO_TABLE_LEN]> = OnceLock::new();

/// Initialize all global lookup tables. Call once before using `Chip`.
pub fn init_tables() {
    MUL_TABLE.get_or_init(|| {
        let mut t = [0u16; 384];
        for i in 0..384 {
            let s = i as i32 * 8;
            t[i] = (0.5 + 2.0_f64.powf(-1.0 + (255 - s) as f64 / 256.0) * (1u32 << MUL_SH) as f64)
                as u16;
        }
        t
    });

    WAVE_TABLE.get_or_init(|| {
        let mut t = [0i16; 4096];
        for i in 0..512 {
            let v = (((i as f64 + 0.5) * (PI / 512.0)).sin() * 4084.0) as i16;
            t[0x200 + i] = v;
            t[0x000 + i] = -v;
        }
        for i in 0..256 {
            let v =
                (0.5 + 2.0_f64.powf(-1.0 + (255 - i as i32 * 8) as f64 / 256.0) * 4085.0) as i16;
            t[0x700 + i] = v;
            if i <= 0x6FF {
                t[0x6FF - i] = -v;
            }
        }
        for i in 0..256 {
            t[0x400 + i] = t[0];
            t[0x500 + i] = t[0];
            t[0x900 + i] = t[0];
            t[0xC00 + i] = t[0];
            t[0xD00 + i] = t[0];
            t[0x800 + i] = t[0x200 + i];
            t[0xA00 + i] = t[0x200 + i * 2];
            t[0xB00 + i] = t[0x000 + i * 2];
            t[0xE00 + i] = t[0x200 + i * 2];
            t[0xF00 + i] = t[0x200 + i * 2];
        }
        t
    });

    KSL_TABLE.get_or_init(|| {
        let mut t = [0u8; 128];
        for oct in 0..8 {
            let base: usize = oct * 8;
            for i in 0..16 {
                let v = base.saturating_sub(KSL_CREATE[i] as usize);
                t[oct * 16 + i] = (v * 4) as u8;
            }
        }
        t
    });

    TREMOLO_DATA.get_or_init(|| {
        let mut t = [0u8; TREMOLO_TABLE_LEN];
        for i in 0..26 {
            t[i] = i as u8;
            t[51 - i] = i as u8;
        }
        t
    });
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn envelope_select(val: u8) -> (usize, u32) {
    if val < 52 {
        ((val & 3) as usize, 12 - (val >> 2) as u32)
    } else if val < 60 {
        ((val - 48) as usize, 0)
    } else {
        (12, 0)
    }
}

#[inline]
fn env_silent(v: i32) -> bool {
    v >= ENV_LIMIT
}

fn wave_tbl() -> &'static [i16; 4096] {
    WAVE_TABLE.get().expect("init_tables not called")
}
fn mul_tbl() -> &'static [u16; 384] {
    MUL_TABLE.get().expect("init_tables not called")
}
fn ksl_tbl() -> &'static [u8; 128] {
    KSL_TABLE.get().expect("init_tables not called")
}
fn trem_tbl() -> &'static [u8; TREMOLO_TABLE_LEN] {
    TREMOLO_DATA.get().expect("init_tables not called")
}

// ── Envelope state ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum EnvState {
    Off = 0,
    Release = 1,
    Sustain = 2,
    Decay = 3,
    Attack = 4,
}

// ── Synthesis mode ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SynthMode {
    Sm2FM,
    Sm2AM,
    Sm2Percussion,
    Sm3FM,
    Sm3AM,
    Sm3FMFM,
    Sm3AMFM,
    Sm3FMAM,
    Sm3AMAM,
    Sm3Percussion,
}

// ── Operator ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Operator {
    wave_base: usize,
    wave_mask: u32,
    wave_start: u32,
    wave_index: u32,
    wave_add: u32,
    wave_current: u32,
    chan_data: u32,
    freq_mul: u32,
    vibrato: u32,
    vib_strength: u8,
    state: EnvState,
    sustain_level: i32,
    total_level: i32,
    current_level: u32,
    volume: i32,
    attack_add: u32,
    decay_add: u32,
    release_add: u32,
    rate_index: u32,
    rate_zero: u8,
    key_on: u8,
    reg20: u8,
    reg40: u8,
    reg60: u8,
    reg80: u8,
    reg_e0: u8,
    tremolo_mask: u8,
    ksr: u8,
}

impl Default for Operator {
    fn default() -> Self {
        Self {
            wave_base: WAVE_BASE_TBL[0] as usize,
            wave_mask: WAVE_MASK_TBL[0] as u32,
            wave_start: (WAVE_START_TBL[0] as u32) << WAVE_SH,
            wave_index: 0,
            wave_add: 0,
            wave_current: 0,
            chan_data: 0,
            freq_mul: 0,
            vibrato: 0,
            vib_strength: 0,
            state: EnvState::Off,
            sustain_level: ENV_MAX,
            total_level: ENV_MAX,
            current_level: ENV_MAX as u32,
            volume: ENV_MAX,
            attack_add: 0,
            decay_add: 0,
            release_add: 0,
            rate_index: 0,
            rate_zero: 1 << (EnvState::Off as u8),
            key_on: 0,
            reg20: 0,
            reg40: 0,
            reg60: 0,
            reg80: 0,
            reg_e0: 0,
            tremolo_mask: 0,
            ksr: 0,
        }
    }
}

impl Operator {
    // ── Rate helpers ────────────────────────────────────────────────────

    fn update_attack(&mut self, attack_rates: &[u32; 76]) {
        let rate = self.reg60 >> 4;
        if rate != 0 {
            let val = ((rate << 2) + self.ksr) as usize;
            self.attack_add = attack_rates[val.min(75)];
            self.rate_zero &= !(1 << EnvState::Attack as u8);
        } else {
            self.attack_add = 0;
            self.rate_zero |= 1 << EnvState::Attack as u8;
        }
    }

    fn update_decay(&mut self, linear_rates: &[u32; 76]) {
        let rate = self.reg60 & 0xF;
        if rate != 0 {
            let val = ((rate << 2) + self.ksr) as usize;
            self.decay_add = linear_rates[val.min(75)];
            self.rate_zero &= !(1 << EnvState::Decay as u8);
        } else {
            self.decay_add = 0;
            self.rate_zero |= 1 << EnvState::Decay as u8;
        }
    }

    fn update_release(&mut self, linear_rates: &[u32; 76]) {
        let rate = self.reg80 & 0xF;
        if rate != 0 {
            let val = ((rate << 2) + self.ksr) as usize;
            self.release_add = linear_rates[val.min(75)];
            self.rate_zero &= !(1 << EnvState::Release as u8);
            if (self.reg20 & MASK_SUSTAIN) == 0 {
                self.rate_zero &= !(1 << EnvState::Sustain as u8);
            }
        } else {
            self.release_add = 0;
            self.rate_zero |= 1 << EnvState::Release as u8;
            if (self.reg20 & MASK_SUSTAIN) == 0 {
                self.rate_zero |= 1 << EnvState::Sustain as u8;
            }
        }
    }

    fn update_rates(&mut self, attack_rates: &[u32; 76], linear_rates: &[u32; 76]) {
        let mut new_ksr = ((self.chan_data >> SHIFT_KEYCODE) & 0xFF) as u8;
        if (self.reg20 & MASK_KSR) == 0 {
            new_ksr >>= 2;
        }
        if self.ksr == new_ksr {
            return;
        }
        self.ksr = new_ksr;
        self.update_attack(attack_rates);
        self.update_decay(linear_rates);
        self.update_release(linear_rates);
    }

    fn update_attenuation(&mut self) {
        let ksl_base = ((self.chan_data >> SHIFT_KSLBASE) & 0xFF) as u8;
        let tl = (self.reg40 & 0x3F) as u32;
        let ksl_shift = KSL_SHIFT[(self.reg40 >> 6) as usize];
        self.total_level = (tl << (ENV_BITS - 7)) as i32;
        self.total_level += (((ksl_base as u32) << ENV_EXTRA) >> ksl_shift) as i32;
    }

    fn update_frequency(&mut self) {
        let freq = self.chan_data & 0x3FF;
        let block = (self.chan_data >> 10) & 0xFF;
        self.wave_add = (freq << block) * self.freq_mul;
        if (self.reg20 & MASK_VIBRATO) != 0 {
            self.vib_strength = (freq >> 7) as u8;
            self.vibrato = ((self.vib_strength as u32) << block) * self.freq_mul;
        } else {
            self.vib_strength = 0;
            self.vibrato = 0;
        }
    }

    // ── Register writes ─────────────────────────────────────────────────

    fn write_20(
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
        self.tremolo_mask = ((val as i8) >> 7) as u8;
        if (change & MASK_KSR) != 0 {
            self.update_rates(attack_rates, linear_rates);
        }
        if (self.reg20 & MASK_SUSTAIN) != 0 || self.release_add == 0 {
            self.rate_zero |= 1 << EnvState::Sustain as u8;
        } else {
            self.rate_zero &= !(1 << EnvState::Sustain as u8);
        }
        if (change & (0x0F | MASK_VIBRATO)) != 0 {
            self.freq_mul = freq_mul[(val & 0xF) as usize];
            self.update_frequency();
        }
    }

    fn write_40(&mut self, val: u8) {
        if (self.reg40 ^ val) == 0 {
            return;
        }
        self.reg40 = val;
        self.update_attenuation();
    }

    fn write_60(&mut self, val: u8, linear_rates: &[u32; 76], attack_rates: &[u32; 76]) {
        let change = self.reg60 ^ val;
        self.reg60 = val;
        if (change & 0x0F) != 0 {
            self.update_decay(linear_rates);
        }
        if (change & 0xF0) != 0 {
            self.update_attack(attack_rates);
        }
    }

    fn write_80(&mut self, val: u8, linear_rates: &[u32; 76]) {
        let change = self.reg80 ^ val;
        if change == 0 {
            return;
        }
        self.reg80 = val;
        let mut sustain = (val >> 4) as u32;
        sustain |= (sustain + 1) & 0x10;
        self.sustain_level = (sustain << (ENV_BITS - 5)) as i32;
        if (change & 0x0F) != 0 {
            self.update_release(linear_rates);
        }
    }

    fn write_e0(&mut self, val: u8, wave_form_mask: u8, opl3_active: bool) {
        if (self.reg_e0 ^ val) == 0 {
            return;
        }
        self.reg_e0 = val;
        let opl3_mask = if opl3_active { 0x7u8 } else { 0u8 };
        let wf = (val & ((0x3 & wave_form_mask) | opl3_mask)) as usize;
        self.wave_base = WAVE_BASE_TBL[wf] as usize;
        self.wave_start = (WAVE_START_TBL[wf] as u32) << WAVE_SH;
        self.wave_mask = WAVE_MASK_TBL[wf] as u32;
    }

    // ── Envelope ────────────────────────────────────────────────────────

    fn set_state(&mut self, s: EnvState) {
        self.state = s;
    }

    fn rate_forward(&mut self, add: u32) -> i32 {
        self.rate_index += add;
        let ret = (self.rate_index >> RATE_SH) as i32;
        self.rate_index &= RATE_MASK;
        ret
    }

    fn template_volume(&mut self) -> i32 {
        let mut vol = self.volume;
        match self.state {
            EnvState::Off => return ENV_MAX,
            EnvState::Attack => {
                let change = self.rate_forward(self.attack_add);
                if change == 0 {
                    return vol;
                }
                vol += ((!vol) * change) >> 3;
                if vol < ENV_MIN {
                    self.volume = ENV_MIN;
                    self.rate_index = 0;
                    self.set_state(EnvState::Decay);
                    return ENV_MIN;
                }
            }
            EnvState::Decay => {
                vol += self.rate_forward(self.decay_add);
                if vol >= self.sustain_level {
                    if vol >= ENV_MAX {
                        self.volume = ENV_MAX;
                        self.set_state(EnvState::Off);
                        return ENV_MAX;
                    }
                    self.rate_index = 0;
                    self.set_state(EnvState::Sustain);
                }
            }
            EnvState::Sustain => {
                if (self.reg20 & MASK_SUSTAIN) != 0 {
                    return vol;
                }
                vol += self.rate_forward(self.release_add);
                if vol >= ENV_MAX {
                    self.volume = ENV_MAX;
                    self.set_state(EnvState::Off);
                    return ENV_MAX;
                }
            }
            EnvState::Release => {
                vol += self.rate_forward(self.release_add);
                if vol >= ENV_MAX {
                    self.volume = ENV_MAX;
                    self.set_state(EnvState::Off);
                    return ENV_MAX;
                }
            }
        }
        self.volume = vol;
        vol
    }

    // ── Sample generation ───────────────────────────────────────────────

    fn forward_volume(&mut self) -> u32 {
        (self.current_level as i32 + self.template_volume()) as u32
    }

    fn forward_wave(&mut self) -> u32 {
        self.wave_index = self.wave_index.wrapping_add(self.wave_current);
        self.wave_index >> WAVE_SH
    }

    fn get_wave(&self, index: u32, vol: u32) -> i32 {
        let wi = self.wave_base + (index & self.wave_mask) as usize;
        let wave = wave_tbl()[wi] as i32;
        let mul = mul_tbl()[(vol >> ENV_EXTRA) as usize] as i32;
        (wave * mul) >> MUL_SH
    }

    fn get_sample(&mut self, modulation: i32) -> i32 {
        let vol = self.forward_volume();
        if env_silent(vol as i32) {
            self.wave_index = self.wave_index.wrapping_add(self.wave_current);
            return 0;
        }
        let index = self.forward_wave();
        let index = (index as i32 + modulation) as u32;
        self.get_wave(index, vol)
    }

    fn prepare(&mut self, chip: &ChipLfo) {
        self.current_level =
            self.total_level as u32 + (chip.tremolo_value as u32 & self.tremolo_mask as u32);
        self.wave_current = self.wave_add;
        if (self.vib_strength >> chip.vibrato_shift) != 0 {
            let mut add = (self.vibrato >> chip.vibrato_shift) as i32;
            let neg = chip.vibrato_sign as i32;
            add = (add ^ neg) - neg;
            self.wave_current = (self.wave_current as i32 + add) as u32;
        }
    }

    fn key_on(&mut self, mask: u8) {
        if self.key_on == 0 {
            self.wave_index = self.wave_start;
            self.rate_index = 0;
            self.set_state(EnvState::Attack);
        }
        self.key_on |= mask;
    }

    fn key_off(&mut self, mask: u8) {
        self.key_on &= !mask;
        if self.key_on == 0 && self.state != EnvState::Off {
            self.set_state(EnvState::Release);
        }
    }

    fn silent(&self) -> bool {
        if !env_silent(self.total_level + self.volume) {
            return false;
        }
        (self.rate_zero & (1 << self.state as u8)) != 0
    }
}

// ── Struct for passing LFO state without borrowing full Chip ────────────────

struct ChipLfo {
    tremolo_value: u8,
    vibrato_sign: i8,
    vibrato_shift: u8,
}

// ── Channel ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Channel {
    op: [Operator; 2],
    chan_data: u32,
    old: [i32; 2],
    feedback: u8,
    reg_b0: u8,
    reg_c0: u8,
    four_mask: u8,
    mask_left: i8,
    mask_right: i8,
    synth_mode: SynthMode,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            op: [Operator::default(), Operator::default()],
            chan_data: 0,
            old: [0, 0],
            feedback: 31,
            reg_b0: 0,
            reg_c0: 0,
            four_mask: 0,
            mask_left: -1,
            mask_right: -1,
            synth_mode: SynthMode::Sm2FM,
        }
    }
}

impl Channel {
    fn set_chan_data(&mut self, data: u32, attack_rates: &[u32; 76], linear_rates: &[u32; 76]) {
        let change = self.chan_data ^ data;
        self.chan_data = data;
        self.op[0].chan_data = data;
        self.op[1].chan_data = data;
        self.op[0].update_frequency();
        self.op[1].update_frequency();
        if (change & (0xFF << SHIFT_KSLBASE)) != 0 {
            self.op[0].update_attenuation();
            self.op[1].update_attenuation();
        }
        if (change & (0xFF << SHIFT_KEYCODE)) != 0 {
            self.op[0].update_rates(attack_rates, linear_rates);
            self.op[1].update_rates(attack_rates, linear_rates);
        }
    }

    fn update_frequency(&mut self, reg08: u8, attack_rates: &[u32; 76], linear_rates: &[u32; 76]) {
        let mut data = self.chan_data & 0xFFFF;
        let ksl_base = ksl_tbl()[(data >> 6) as usize] as u32;
        let mut key_code = (data & 0x1C00) >> 9;
        if (reg08 & 0x40) != 0 {
            key_code |= (data & 0x100) >> 8;
        } else {
            key_code |= (data & 0x200) >> 9;
        }
        data |= (key_code << SHIFT_KEYCODE) | (ksl_base << SHIFT_KSLBASE);
        self.set_chan_data(data, attack_rates, linear_rates);
    }

    fn write_a0(
        &mut self,
        val: u8,
        reg08: u8,
        attack_rates: &[u32; 76],
        linear_rates: &[u32; 76],
        opl3_active: bool,
    ) {
        let four_op_raw = if opl3_active { self.four_mask } else { 0 };
        if four_op_raw > 0x80 {
            return;
        }
        let change = (self.chan_data ^ val as u32) & 0xFF;
        if change != 0 {
            self.chan_data ^= change;
            self.update_frequency(reg08, attack_rates, linear_rates);
        }
    }

    fn write_b0(
        &mut self,
        val: u8,
        reg08: u8,
        attack_rates: &[u32; 76],
        linear_rates: &[u32; 76],
        opl3_active: bool,
    ) {
        let four_op_raw = if opl3_active { self.four_mask } else { 0 };
        if four_op_raw > 0x80 {
            return;
        }
        let change = (self.chan_data ^ ((val as u32) << 8)) & 0x1F00;
        if change != 0 {
            self.chan_data ^= change;
            self.update_frequency(reg08, attack_rates, linear_rates);
        }
        if ((val ^ self.reg_b0) & 0x20) == 0 {
            return;
        }
        self.reg_b0 = val;
        if (val & 0x20) != 0 {
            self.op[0].key_on(0x1);
            self.op[1].key_on(0x1);
        } else {
            self.op[0].key_off(0x1);
            self.op[1].key_off(0x1);
        }
    }

    fn write_c0(&mut self, val: u8, opl3_active: bool, reg_bd: u8) {
        let change = val ^ self.reg_c0;
        if change == 0 {
            return;
        }
        self.reg_c0 = val;
        let fb = (val >> 1) & 7;
        self.feedback = if fb != 0 { 9 - fb } else { 31 };

        if opl3_active {
            if (self.four_mask & 0x40) != 0 && (reg_bd & 0x20) != 0 {
                // percussion — no synth change
            } else if (val & 1) != 0 {
                self.synth_mode = SynthMode::Sm3AM;
            } else {
                self.synth_mode = SynthMode::Sm3FM;
            }
            self.mask_left = if (val & 0x10) != 0 { -1 } else { 0 };
            self.mask_right = if (val & 0x20) != 0 { -1 } else { 0 };
        } else {
            if (self.four_mask & 0x40) != 0 && (reg_bd & 0x20) != 0 {
                // percussion
            } else if (val & 1) != 0 {
                self.synth_mode = SynthMode::Sm2AM;
            } else {
                self.synth_mode = SynthMode::Sm2FM;
            }
        }
    }

    fn generate(&mut self, lfo: &ChipLfo, samples: usize, output: &mut [i32]) {
        let mode = self.synth_mode;
        // Silent check
        match mode {
            SynthMode::Sm2AM | SynthMode::Sm3AM => {
                if self.op[0].silent() && self.op[1].silent() {
                    self.old = [0, 0];
                    return;
                }
            }
            SynthMode::Sm2FM | SynthMode::Sm3FM => {
                if self.op[1].silent() {
                    self.old = [0, 0];
                    return;
                }
            }
            SynthMode::Sm2Percussion | SynthMode::Sm3Percussion => {
                // percussion always generates
            }
            _ => {
                // 4-op modes not needed for OPL2
                return;
            }
        }

        self.op[0].prepare(lfo);
        self.op[1].prepare(lfo);

        for i in 0..samples {
            let mod_val = (self.old[0] as u32).wrapping_add(self.old[1] as u32) >> self.feedback;
            self.old[0] = self.old[1];
            self.old[1] = self.op[0].get_sample(mod_val as i32);
            let out0 = self.old[0];

            let sample = match mode {
                SynthMode::Sm2AM | SynthMode::Sm3AM => out0 + self.op[1].get_sample(0),
                SynthMode::Sm2FM | SynthMode::Sm3FM => self.op[1].get_sample(out0),
                _ => 0,
            };

            match mode {
                SynthMode::Sm2AM | SynthMode::Sm2FM => {
                    output[i] += sample;
                }
                SynthMode::Sm3AM | SynthMode::Sm3FM => {
                    output[i * 2] += sample & self.mask_left as i32;
                    output[i * 2 + 1] += sample & self.mask_right as i32;
                }
                _ => {}
            }
        }
    }
}

// ── Chip ────────────────────────────────────────────────────────────────────

/// OPL2/OPL3 FM synthesis chip emulator
pub struct Chip {
    chan: [Channel; 18],
    freq_mul: [u32; 16],
    linear_rates: [u32; 76],
    attack_rates: [u32; 76],
    lfo_counter: u32,
    lfo_add: u32,
    noise_counter: u32,
    noise_add: u32,
    noise_value: u32,
    vibrato_index: u8,
    tremolo_index: u8,
    vibrato_sign: i8,
    vibrato_shift: u8,
    vibrato_strength: u8,
    tremolo_value: u8,
    tremolo_strength: u8,
    wave_form_mask: u8,
    opl3_active: bool,
    reg08: u8,
    reg_bd: u8,
    reg104: u8,
}

impl Chip {
    /// Create a new chip. Call `setup(rate)` before use.
    pub fn new() -> Self {
        let mut chip = Self {
            chan: std::array::from_fn(|_| Channel::default()),
            freq_mul: [0; 16],
            linear_rates: [0; 76],
            attack_rates: [0; 76],
            lfo_counter: 0,
            lfo_add: 0,
            noise_counter: 0,
            noise_add: 0,
            noise_value: 1,
            vibrato_index: 0,
            tremolo_index: 0,
            vibrato_sign: 0,
            vibrato_shift: 0,
            vibrato_strength: 1,
            tremolo_value: 0,
            tremolo_strength: 2,
            wave_form_mask: 0,
            opl3_active: false,
            reg08: 0,
            reg_bd: 0,
            reg104: 0,
        };
        // Set four_mask for percussion channels
        chip.chan[6].four_mask = 0x40;
        chip.chan[7].four_mask = 0x40;
        chip.chan[8].four_mask = 0x40;
        chip
    }

    /// Initialize rate tables for the given output sample rate
    pub fn setup(&mut self, rate: u32) {
        let scale = OPLRATE / rate as f64;

        self.noise_add = (0.5 + scale * (1u32 << LFO_SH) as f64) as u32;
        self.noise_counter = 0;
        self.noise_value = 1;

        self.lfo_add = (0.5 + scale * (1u32 << LFO_SH) as f64) as u32;
        self.lfo_counter = 0;
        self.vibrato_index = 0;
        self.tremolo_index = 0;

        let freq_scale = (0.5 + scale * (1u64 << (WAVE_SH - 1 - 10)) as f64) as u32;
        for i in 0..16 {
            self.freq_mul[i] = freq_scale * FREQ_CREATE[i] as u32;
        }

        for i in 0..76 {
            let (idx, sh) = envelope_select(i as u8);
            self.linear_rates[i] = (scale
                * ((ENV_INCREASE[idx] as u32) << (RATE_SH + ENV_EXTRA - sh - 3)) as f64)
                as u32;
        }

        for i in 0..62 {
            let (idx, sh) = envelope_select(i as u8);
            let original = ((ATTACK_SAMPLES[idx] as u32) << sh) as f64 / scale;
            let original = original as i32;

            let mut guess =
                (scale * ((ENV_INCREASE[idx] as u32) << (RATE_SH - sh - 3)) as f64) as u32;
            let mut best = guess;
            let mut best_diff: i32 = 1 << 30;

            for _ in 0..16 {
                let mut volume: i32 = ENV_MAX;
                let mut samples: i32 = 0;
                let mut count: u32 = 0;
                while volume > 0 && samples < original * 2 {
                    count += guess;
                    let change = count >> RATE_SH;
                    count &= RATE_MASK;
                    if change != 0 {
                        volume += (!volume * change as i32) >> 3;
                    }
                    samples += 1;
                }
                let diff = original - samples;
                let l_diff = diff.abs();
                if l_diff < best_diff {
                    best_diff = l_diff;
                    best = guess;
                    if best_diff == 0 {
                        break;
                    }
                }
                if diff < 0 {
                    let mul = ((original - diff) << 12) / original;
                    guess = ((guess as i64 * mul as i64) >> 12) as u32;
                    guess += 1;
                } else if diff > 0 {
                    let mul = ((original - diff) << 12) / original;
                    guess = ((guess as i64 * mul as i64) >> 12) as u32;
                    guess = guess.wrapping_sub(1);
                }
            }
            self.attack_rates[i] = best;
        }
        for i in 62..76 {
            self.attack_rates[i] = 8 << RATE_SH;
        }

        self.vibrato_strength = 1;
        self.tremolo_strength = 2;
    }

    /// Write a value to an OPL register
    pub fn write_reg(&mut self, reg: u32, val: u8) {
        match (reg & 0xF0) >> 4 {
            0x0 => {
                if reg == 0x01 {
                    self.wave_form_mask = if (val & 0x20) != 0 { 0x7 } else { 0x0 };
                } else if reg == 0x08 {
                    self.reg08 = val;
                }
            }
            0x1 => {}
            0x2 | 0x3 => {
                let index = (((reg >> 3) & 0x20) | (reg & 0x1F)) as usize;
                if let Some((ch, op)) = Self::op_index(index) {
                    let fm = self.freq_mul;
                    let ar = self.attack_rates;
                    let lr = self.linear_rates;
                    self.chan[ch].op[op].write_20(val, &fm, &ar, &lr);
                }
            }
            0x4 | 0x5 => {
                let index = (((reg >> 3) & 0x20) | (reg & 0x1F)) as usize;
                if let Some((ch, op)) = Self::op_index(index) {
                    self.chan[ch].op[op].write_40(val);
                }
            }
            0x6 | 0x7 => {
                let index = (((reg >> 3) & 0x20) | (reg & 0x1F)) as usize;
                if let Some((ch, op)) = Self::op_index(index) {
                    let lr = self.linear_rates;
                    let ar = self.attack_rates;
                    self.chan[ch].op[op].write_60(val, &lr, &ar);
                }
            }
            0x8 | 0x9 => {
                let index = (((reg >> 3) & 0x20) | (reg & 0x1F)) as usize;
                if let Some((ch, op)) = Self::op_index(index) {
                    let lr = self.linear_rates;
                    self.chan[ch].op[op].write_80(val, &lr);
                }
            }
            0xA => {
                let index = (((reg >> 4) & 0x10) | (reg & 0xF)) as usize;
                let ch = Self::chan_index(index);
                if ch < 18 {
                    let reg08 = self.reg08;
                    let ar = self.attack_rates;
                    let lr = self.linear_rates;
                    let opl3 = self.opl3_active;
                    self.chan[ch].write_a0(val, reg08, &ar, &lr, opl3);
                }
            }
            0xB => {
                if reg == 0xBD {
                    self.write_bd(val);
                } else {
                    let index = (((reg >> 4) & 0x10) | (reg & 0xF)) as usize;
                    let ch = Self::chan_index(index);
                    if ch < 18 {
                        let reg08 = self.reg08;
                        let ar = self.attack_rates;
                        let lr = self.linear_rates;
                        let opl3 = self.opl3_active;
                        self.chan[ch].write_b0(val, reg08, &ar, &lr, opl3);
                    }
                }
            }
            0xC => {
                let index = (((reg >> 4) & 0x10) | (reg & 0xF)) as usize;
                let ch = Self::chan_index(index);
                if ch < 18 {
                    let opl3 = self.opl3_active;
                    let bd = self.reg_bd;
                    self.chan[ch].write_c0(val, opl3, bd);
                }
            }
            0xD => {}
            0xE | 0xF => {
                let index = (((reg >> 3) & 0x20) | (reg & 0x1F)) as usize;
                if let Some((ch, op)) = Self::op_index(index) {
                    let wfm = self.wave_form_mask;
                    let opl3 = self.opl3_active;
                    self.chan[ch].op[op].write_e0(val, wfm, opl3);
                }
            }
            _ => {}
        }
    }

    /// Generate a block of mono audio samples (OPL2 mode)
    pub fn generate_block_2(&mut self, total: usize, output: &mut [i32]) {
        output[..total].fill(0);
        let mut out_off = 0;
        let mut remaining = total;

        while remaining > 0 {
            let samples = self.forward_lfo(remaining as u32) as usize;
            let lfo = ChipLfo {
                tremolo_value: self.tremolo_value,
                vibrato_sign: self.vibrato_sign,
                vibrato_shift: self.vibrato_shift,
            };
            for ch_idx in 0..9 {
                let chan = &mut self.chan[ch_idx];
                chan.generate(&lfo, samples, &mut output[out_off..]);
            }
            remaining -= samples;
            out_off += samples;
        }
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn forward_lfo(&mut self, samples: u32) -> u32 {
        let vi = (self.vibrato_index as usize >> 2) & 7;
        self.vibrato_sign = VIBRATO_TBL[vi] >> 7;
        self.vibrato_shift = (VIBRATO_TBL[vi] & 7) as u8 + self.vibrato_strength;
        self.tremolo_value = trem_tbl()[self.tremolo_index as usize] >> self.tremolo_strength;

        let todo = LFO_MAX - self.lfo_counter;
        let mut count = (todo + self.lfo_add - 1) / self.lfo_add;

        // Guard against zero to prevent infinite loops when called with total=1
        if count == 0 {
            count = 1;
        }

        if count > samples {
            count = samples;
            self.lfo_counter += count * self.lfo_add;
        } else {
            self.lfo_counter += count * self.lfo_add;
            self.lfo_counter &= LFO_MAX - 1;
            self.vibrato_index = (self.vibrato_index + 1) & 31;
            if (self.tremolo_index as usize + 1) < TREMOLO_TABLE_LEN {
                self.tremolo_index += 1;
            } else {
                self.tremolo_index = 0;
            }
        }
        count
    }

    fn write_bd(&mut self, val: u8) {
        let change = self.reg_bd ^ val;
        if change == 0 {
            return;
        }
        self.reg_bd = val;
        self.vibrato_strength = if (val & 0x40) != 0 { 0 } else { 1 };
        self.tremolo_strength = if (val & 0x80) != 0 { 0 } else { 2 };

        if (change & 0x20) != 0 {
            if (val & 0x20) != 0 {
                self.chan[6].synth_mode = SynthMode::Sm2Percussion;
                self.chan[7].synth_mode = SynthMode::Sm2Percussion;
                self.chan[8].synth_mode = SynthMode::Sm2Percussion;
            } else {
                let opl3 = self.opl3_active;
                let bd = self.reg_bd;
                self.chan[6].write_c0(self.chan[6].reg_c0 ^ 0xFF, opl3, bd);
                self.chan[6].write_c0(self.chan[6].reg_c0 ^ 0xFF, opl3, bd);
                self.chan[7].write_c0(self.chan[7].reg_c0 ^ 0xFF, opl3, bd);
                self.chan[7].write_c0(self.chan[7].reg_c0 ^ 0xFF, opl3, bd);
                self.chan[8].write_c0(self.chan[8].reg_c0 ^ 0xFF, opl3, bd);
                self.chan[8].write_c0(self.chan[8].reg_c0 ^ 0xFF, opl3, bd);
            }
        }

        // Percussion key on/off (mask 0x2)
        macro_rules! perc_key {
            ($bit:expr, $ch:expr, $op:expr) => {
                if (change & $bit) != 0 {
                    if (val & $bit) != 0 {
                        self.chan[$ch].op[$op].key_on(0x2);
                    } else {
                        self.chan[$ch].op[$op].key_off(0x2);
                    }
                }
            };
        }
        perc_key!(0x10, 6, 0); // BD mod
        perc_key!(0x10, 6, 1); // BD car
        perc_key!(0x01, 7, 0); // HH
        perc_key!(0x08, 7, 1); // SD
        perc_key!(0x04, 8, 0); // TT
        perc_key!(0x02, 8, 1); // TC
    }

    /// Maps register index to (channel, operator) pair
    fn op_index(index: usize) -> Option<(usize, usize)> {
        let group = index / 8;
        let slot = index % 8;
        if slot >= 6 || group % 4 == 3 {
            return None;
        }
        let mut ch = group * 3 + slot % 3;
        let op = slot / 3;
        if ch >= 12 {
            ch += 4;
        }
        if ch < 18 && op < 2 {
            Some((ch, op))
        } else {
            None
        }
    }

    /// Maps register index to channel index
    fn chan_index(index: usize) -> usize {
        match index {
            0..=8 => index,
            16..=24 => index - 16 + 9,
            _ => 18, // invalid
        }
    }
}

impl Default for Chip {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_tables() {
        init_tables();
        assert!(WAVE_TABLE.get().is_some());
        assert!(MUL_TABLE.get().is_some());
        assert!(KSL_TABLE.get().is_some());
        assert!(TREMOLO_DATA.get().is_some());
    }

    #[test]
    fn test_envelope_select() {
        assert_eq!(envelope_select(0), (0, 12));
        assert_eq!(envelope_select(4), (0, 11));
        assert_eq!(envelope_select(51), (3, 0));
        assert_eq!(envelope_select(52), (4, 0));
        assert_eq!(envelope_select(59), (11, 0));
        assert_eq!(envelope_select(60), (12, 0));
    }

    #[test]
    fn test_op_index_mapping() {
        // Channel 0: mod=slot0, car=slot3
        assert_eq!(Chip::op_index(0), Some((0, 0)));
        assert_eq!(Chip::op_index(3), Some((0, 1)));
        // Channel 1
        assert_eq!(Chip::op_index(1), Some((1, 0)));
        assert_eq!(Chip::op_index(4), Some((1, 1)));
        // Channel 3 (group 1)
        assert_eq!(Chip::op_index(8), Some((3, 0)));
        assert_eq!(Chip::op_index(11), Some((3, 1)));
        // Invalid slots
        assert_eq!(Chip::op_index(6), None);
        assert_eq!(Chip::op_index(7), None);
        // Group 3 invalid
        assert_eq!(Chip::op_index(24), None);
    }

    #[test]
    fn test_chip_setup_and_generate() {
        init_tables();
        let mut chip = Chip::new();
        chip.setup(44100);
        let mut output = [0i32; 64];
        chip.generate_block_2(64, &mut output);
        // Should produce silence with no active notes
        assert!(output.iter().all(|&s| s == 0));
    }

    #[test]
    fn test_basic_tone() {
        init_tables();
        let mut chip = Chip::new();
        chip.setup(44100);

        chip.write_reg(0x01, 0x20); // enable waveform select
        chip.write_reg(0x20, 0x01); // mod: freq mul 1
        chip.write_reg(0x23, 0x01); // car: freq mul 1
        chip.write_reg(0x40, 0x10); // mod: level
        chip.write_reg(0x43, 0x00); // car: level 0 (loudest)
        chip.write_reg(0x60, 0xF0); // mod: attack=F, decay=0
        chip.write_reg(0x63, 0xF0); // car: attack=F, decay=0
        chip.write_reg(0x80, 0x77); // mod: sustain=7, release=7
        chip.write_reg(0x83, 0x77); // car: sustain=7, release=7
        chip.write_reg(0xC0, 0x30); // feedback=0, FM mode, both speakers
        chip.write_reg(0xA0, 0x98); // freq low
        chip.write_reg(0xB0, 0x31); // freq high + key on

        let mut output = [0i32; 512];
        chip.generate_block_2(512, &mut output);
        // Should produce non-zero output
        let max = output.iter().map(|s| s.abs()).max().unwrap_or(0);
        assert!(max > 0, "Expected non-zero output, got silence");
    }
}
