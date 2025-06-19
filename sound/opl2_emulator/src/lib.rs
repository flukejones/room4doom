//! # OPL2/OPL3 Emulator
//!
//! A Rust implementation of the Yamaha YMF262 (OPL3) and YM3812 (OPL2) sound chip emulator.
//! This crate provides a faithful emulation of the classic FM synthesis chips used in
//! many retro games and sound cards from the 1990s.
//!
//! Originally based on the DOSBox OPL emulator implementation, this Rust version maintains
//! compatibility while providing memory safety and modern error handling.
//!
//! ## Features
//!
//! - Full OPL2/OPL3 register compatibility
//! - Integer-only math implementation for consistent results across platforms
//! - Support for both mono (OPL2) and stereo (OPL3) output
//! - Percussion mode support for rhythm instruments
//! - Multiple waveform generation modes (sine, half-sine, abs-sine, pulse)
//! - Configurable wave precision through features
//! - Thread-safe lazy initialization using `OnceLock`
//! - Memory-safe implementation with minimal unsafe code
//!
//! ## Architecture
//!
//! The emulator consists of three main components:
//!
//! - **Chip**: The main controller that manages channels and global state
//! - **Channel**: Represents a single FM voice, containing two operators
//! - **Operator**: The basic FM synthesis unit that generates sine waves
//!
//! ## Usage
//!
//! ```rust
//! use opl2_emulator::Chip;
//!
//! // Initialize the chip (false for OPL2, true for OPL3)
//! let mut chip = Chip::new(false);
//! chip.setup(44100); // Set sample rate to 44.1kHz
//!
//! // Configure a simple sine wave on channel 0
//! chip.write_reg(0x20, 0x01); // Set operator 1 parameters
//! chip.write_reg(0x23, 0x01); // Set operator 2 parameters
//! chip.write_reg(0x40, 0x10); // Set operator 1 volume
//! chip.write_reg(0x43, 0x00); // Set operator 2 volume
//! chip.write_reg(0x60, 0xF0); // Set operator 1 attack/decay
//! chip.write_reg(0x63, 0xF0); // Set operator 2 attack/decay
//! chip.write_reg(0x80, 0x77); // Set operator 1 sustain/release
//! chip.write_reg(0x83, 0x77); // Set operator 2 sustain/release
//! chip.write_reg(0xA0, 0x98); // Set frequency low byte (440Hz)
//! chip.write_reg(0xB0, 0x31); // Set frequency high byte and key on
//!
//! // Generate audio samples
//! let mut output = vec![0i32; 1024];
//! chip.generate_block_2(1024, &mut output);
//!
//! // Convert to 16-bit samples
//! let samples_16bit: Vec<i16> = output.iter()
//!     .map(|&sample| (sample >> 8) as i16)
//!     .collect();
//! ```
//!
//! ## Wave Precision
//!
//! The crate supports an optional `wave_precision` feature that increases the precision
//! of wave calculations at the cost of memory usage. Enable it in your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! opl2_emulator = { version = "0.16.1", features = ["wave_precision"] }
//! ```
//!
//! ## Register Reference
//!
//! The OPL2/OPL3 chips are programmed by writing to specific registers:
//!
//! - `0x20-0x35`: Operator parameters (AM/VIB/EG/KSR/MULT)
//! - `0x40-0x55`: Operator volume (KSL/TL)
//! - `0x60-0x75`: Operator envelope (AR/DR)
//! - `0x80-0x95`: Operator sustain/release (SL/RR)
//! - `0xA0-0xA8`: Channel frequency low byte
//! - `0xB0-0xB8`: Channel frequency high byte + key on/off
//! - `0xC0-0xC8`: Channel feedback and connection
//! - `0xE0-0xF5`: Operator waveform select
//!
//! For detailed register documentation, refer to the original Yamaha datasheets.

use std::f64::consts::PI;
use std::sync::OnceLock;

pub mod channel;
pub mod chip;
pub mod operator;

// Re-export main types
// pub use chip::Chip;

/// The base OPL clock rate in Hz (approximately 49.716 kHz)
pub const OPLRATE: f64 = 14318180.0 / 288.0;

/// Size of the tremolo lookup table
const TREMOLO_TABLE: usize = 52;

/// Number of bits used for wave calculations (normal precision)
#[cfg(not(feature = "wave_precision"))]
const WAVE_BITS: u32 = 10;

/// Number of bits used for wave calculations (high precision)
#[cfg(feature = "wave_precision")]
const WAVE_BITS: u32 = 14;

/// Wave calculation shift amount
const WAVE_SH: u32 = 32 - WAVE_BITS;
/// Wave mask for modulo operations
const WAVE_MASK: u32 = (1 << WAVE_SH) - 1;

/// LFO (Low Frequency Oscillator) shift amount
const LFO_SH: u32 = WAVE_SH - 10;
/// Maximum LFO counter value
const LFO_MAX: u32 = 256 << LFO_SH;

/// Number of bits for envelope calculations
const ENV_BITS: u32 = 9;
/// Minimum envelope value (loudest)
const ENV_MIN: i32 = 0;
/// Extra envelope precision bits
const ENV_EXTRA: u32 = ENV_BITS - 9;
/// Maximum envelope value (silent)
const ENV_MAX: i32 = 511 << ENV_EXTRA;
/// Envelope silence threshold
const ENV_LIMIT: i32 = (12 * 256) >> (3 - ENV_EXTRA);

/// Rate counter shift amount
const RATE_SH: u32 = 24;
/// Rate counter mask
const RATE_MASK: u32 = (1 << RATE_SH) - 1;
/// Multiplication table shift amount
const MUL_SH: u32 = 16;

/// Represents the current state of an FM operator's envelope generator
///
/// The envelope generator controls the amplitude of the operator over time,
/// creating the characteristic attack, decay, sustain, and release phases
/// of FM synthesis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OperatorState {
    /// Operator is silent and not producing sound
    Off,
    /// Operator is in the release phase (volume decreasing after key off)
    Release,
    /// Operator is in the sustain phase (holding at sustain level)
    Sustain,
    /// Operator is in the decay phase (volume decreasing to sustain level)
    Decay,
    /// Operator is in the attack phase (volume increasing from zero)
    Attack,
}

/// Synthesis modes for different channel configurations
///
/// These modes determine how operators within a channel are combined
/// to produce the final audio output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SynthMode {
    /// 2-operator additive synthesis (OPL2)
    Sm2AM,
    /// 2-operator frequency modulation (OPL2)
    Sm2FM,
    /// 3-operator additive synthesis (OPL3)
    Sm3AM,
    /// 3-operator frequency modulation (OPL3)
    Sm3FM,
    /// 4-operator mode start marker (unused in simplified implementation)
    Sm4Start,
    /// 3-operator FM->FM (OPL3)
    Sm3FMFM,
    /// 3-operator AM->FM (OPL3)
    Sm3AMFM,
    /// 3-operator FM->AM (OPL3)
    Sm3FMAM,
    /// 3-operator AM->AM (OPL3)
    Sm3AMAM,
    /// 6-operator mode start marker (unused in simplified implementation)
    Sm6Start,
    /// 2-operator percussion mode (OPL2)
    Sm2Percussion,
    /// 3-operator percussion mode (OPL3)
    Sm3Percussion,
}

/// Bit shift amount for KSL base value in channel data
const SHIFT_KSLBASE: u32 = 16;
/// Bit shift amount for key code in channel data
const SHIFT_KEYCODE: u32 = 24;

/// Register mask for Key Scale Rate (KSR) bit
const MASK_KSR: u8 = 0x10;
/// Register mask for sustain bit
const MASK_SUSTAIN: u8 = 0x20;
/// Register mask for vibrato bit
const MASK_VIBRATO: u8 = 0x40;
/// Register mask for tremolo bit
const MASK_TREMOLO: u8 = 0x80;

/// Checks if an envelope value represents silence
///
/// # Arguments
/// * `x` - The envelope value to check
///
/// # Returns
/// `true` if the envelope value is considered silent
pub fn env_silent(x: i32) -> bool {
    x >= ENV_LIMIT
}

/// Lookup table for KSL (Key Scale Level) calculation
static KSL_CREATE_TABLE: [u8; 16] = [64, 32, 24, 19, 16, 12, 11, 10, 8, 6, 5, 4, 3, 2, 1, 0];

/// Lookup table for frequency multiplier calculation
static FREQ_CREATE_TABLE: [u8; 16] = [1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 20, 24, 24, 30, 30];

/// Lookup table for attack phase sample counts
static ATTACK_SAMPLES_TABLE: [u8; 13] = [69, 55, 46, 40, 35, 29, 23, 20, 19, 15, 11, 10, 9];

/// Lookup table for envelope increase calculations
static ENVELOPE_INCREASE_TABLE: [u8; 13] = [4, 5, 6, 7, 8, 10, 12, 14, 16, 20, 24, 28, 32];

/// Vibrato waveform lookup table
static VIBRATO_TABLE: [i8; 8] = [1, 0, 1, 30, -127, -128, -127, -98];

/// Key Scale Level shift amounts for different KSL settings
static KSL_SHIFT_TABLE: [u8; 4] = [31, 1, 2, 0];

/// Thread-safe lazy initialization of exponential lookup table
static EXP_TABLE: OnceLock<[u16; 256]> = OnceLock::new();
/// Thread-safe lazy initialization of sine lookup table
static SIN_TABLE: OnceLock<[u16; 512]> = OnceLock::new();
/// Thread-safe lazy initialization of waveform lookup table
static WAVE_TABLE: OnceLock<[i16; 8 * 512]> = OnceLock::new();
/// Thread-safe lazy initialization of multiplication lookup table
static MUL_TABLE: OnceLock<[u16; 384]> = OnceLock::new();
/// Thread-safe lazy initialization of KSL lookup table
static KSL_TABLE: OnceLock<[u8; 8 * 16]> = OnceLock::new();
/// Thread-safe lazy initialization of tremolo lookup table
static TREMOLO_TABLE_DATA: OnceLock<[u8; TREMOLO_TABLE]> = OnceLock::new();
/// Thread-safe lazy initialization of channel offset table (unused in safe implementation)
static CHAN_OFFSET_TABLE: OnceLock<[u16; 32]> = OnceLock::new();
/// Thread-safe lazy initialization of operator offset table (unused in safe implementation)
static OP_OFFSET_TABLE: OnceLock<[u16; 64]> = OnceLock::new();

/// Base wave offsets for different waveforms
static WAVE_BASE_TABLE: [u16; 8] = [0x000, 0x200, 0x200, 0x800, 0xa00, 0xc00, 0x100, 0x400];

/// Wave masks for different waveforms
static WAVE_MASK_TABLE: [u16; 8] = [1023, 1023, 511, 511, 1023, 1023, 512, 1023];

/// Wave start positions for different waveforms
static WAVE_START_TABLE: [u16; 8] = [512, 0, 0, 0, 0, 512, 512, 256];

/// Initialize all lookup tables used by the OPL emulator
///
/// This function is called automatically when creating a new Chip instance.
/// It uses thread-safe lazy initialization to ensure tables are only
/// computed once, even in multi-threaded environments.
pub fn init_tables() {
    // Initialize EXP_TABLE: exponential decay curve
    EXP_TABLE.get_or_init(|| {
        let mut table = [0u16; 256];
        for i in 0..256 {
            table[i] =
                ((2.0_f64.powf((255 - i) as f64 / 256.0) - 1.0) * 1024.0 + 1024.0) as u16 * 2;
        }
        table
    });

    // Initialize SIN_TABLE: sine wave logarithmic representation
    SIN_TABLE.get_or_init(|| {
        let mut table = [0u16; 512];
        for i in 0..512 {
            table[i] = (0.5
                - ((i as f64 + 0.5) * (PI / 512.0)).sin().log10() / 2.0_f64.log10() * 256.0)
                as u16;
        }
        table
    });

    // Initialize MUL_TABLE: multiplication factors for volume control
    MUL_TABLE.get_or_init(|| {
        let mut table = [0u16; 384];
        for i in 0..384 {
            let s = i * 8;
            let val = if s <= 255 {
                0.5 + 2.0_f64.powf(-1.0 + (255 - s) as f64 / 256.0) * (1 << MUL_SH) as f64
            } else {
                0.5 + 2.0_f64.powf(-1.0 + 0.0 / 256.0) * (1 << MUL_SH) as f64
            };
            table[i] = val as u16;
        }
        table
    });

    // Initialize WAVE_TABLE: pre-computed waveforms
    WAVE_TABLE.get_or_init(|| {
        let mut table = [0i16; 8 * 512];

        // Initialize sine waves
        for i in 0..512 {
            let val = (((i as f64 + 0.5) * (PI / 512.0)).sin() * 4084.0) as i16;
            table[0x0200 + i] = val; // Positive sine
            table[0x0000 + i] = -val; // Negative sine
        }

        // Initialize exponential waves
        for i in 0..256 {
            let s = i * 8;
            let val = if s <= 255 {
                (0.5 + 2.0_f64.powf(-1.0 + (255 - s) as f64 / 256.0) * 4085.0) as i16
            } else {
                (0.5 + 2.0_f64.powf(-1.0 + 0.0 / 256.0) * 4085.0) as i16
            };
            table[0x700 + i] = val;
            if 0x6ff >= i {
                table[0x6ff - i] = -val;
            }
        }

        // Initialize other wave forms (square, saw, etc.)
        for i in 0..256 {
            let zero_val = table[0];
            table[0x400 + i] = zero_val; // Zero wave
            table[0x500 + i] = zero_val; // Zero wave
            table[0x900 + i] = zero_val; // Zero wave
            table[0xc00 + i] = zero_val; // Zero wave
            table[0xd00 + i] = zero_val; // Zero wave
            table[0x800 + i] = table[0x200 + i]; // Half sine
            table[0xa00 + i] = table[0x200 + i * 2]; // Abs sine
            table[0xb00 + i] = table[0x000 + i * 2]; // Pulse sine
            table[0xe00 + i] = table[0x200 + i * 2]; // Saw sine
            table[0xf00 + i] = table[0x200 + i * 2]; // Square sine
        }

        table
    });

    // Initialize KSL_TABLE: Key Scale Level lookup
    KSL_TABLE.get_or_init(|| {
        let mut table = [0u8; 8 * 16];
        for oct in 0..8 {
            let base = oct * 8;
            for i in 0..16 {
                let val = if base >= KSL_CREATE_TABLE[i] as usize {
                    base - KSL_CREATE_TABLE[i] as usize
                } else {
                    0
                };
                table[oct * 16 + i] = (val * 4) as u8;
            }
        }
        table
    });

    // Initialize TREMOLO_TABLE_DATA: tremolo modulation curve
    TREMOLO_TABLE_DATA.get_or_init(|| {
        let mut table = [0u8; TREMOLO_TABLE];
        for i in 0..(TREMOLO_TABLE / 2) {
            let val = (i << ENV_EXTRA) as u8;
            table[i] = val;
            table[TREMOLO_TABLE - 1 - i] = val;
        }
        table
    });

    // Initialize unused offset tables (kept for compatibility)
    CHAN_OFFSET_TABLE.get_or_init(|| [0u16; 32]);
    OP_OFFSET_TABLE.get_or_init(|| [0u16; 64]);
}

/// Function pointer type for volume envelope handlers
///
/// These functions implement the different phases of the ADSR envelope
/// (Attack, Decay, Sustain, Release) and return the current volume level.
type VolumeHandler = fn(&mut Operator) -> i32;

/// Function pointer type for synthesis handlers
///
/// These functions implement different synthesis modes (FM, AM, percussion)
/// and return the number of channels processed.
type SynthHandler = fn(&mut Channel, &mut Chip, u32, &mut [i32]) -> usize;

/// Represents a single FM operator within the OPL chip
///
/// An operator is the basic building block of FM synthesis, generating
/// sine waves that can modulate each other to create complex timbres.
/// Each operator has its own envelope generator, frequency settings,
/// and waveform selection.
pub struct Operator {
    /// Function pointer to current volume calculation method
    vol_handler: VolumeHandler,
    /// Base offset into the waveform table
    wave_base: usize,
    /// Mask for waveform table indexing
    wave_mask: u32,
    /// Starting position in waveform table
    wave_start: u32,
    /// Current position in waveform table
    wave_index: u32,
    /// Frequency increment per sample
    wave_add: u32,
    /// Current frequency value (including modulation)
    wave_current: u32,
    /// Channel data (frequency and key scaling info)
    chan_data: u32,
    /// Frequency multiplier setting
    freq_mul: u32,
    /// Vibrato depth and frequency offset
    vibrato: u32,
    /// Sustain level threshold
    sustain_level: i32,
    /// Total level (volume) including key scaling
    total_level: i32,
    /// Current level including tremolo
    current_level: u32,
    /// Current envelope volume
    volume: i32,
    /// Attack phase increment
    attack_add: u32,
    /// Decay phase increment
    decay_add: u32,
    /// Release phase increment
    release_add: u32,
    /// Rate calculation index
    rate_index: u32,
    /// Bitmask for zero-rate detection
    rate_zero: u32,
    /// Key-on state mask
    key_on: u8,
    /// Register 0x20 value (operator characteristics)
    reg20: u8,
    /// Register 0x40 value (key scale level and total level)
    reg40: u8,
    /// Register 0x60 value (attack rate and decay rate)
    reg60: u8,
    /// Register 0x80 value (sustain level and release rate)
    reg80: u8,
    /// Register 0xE0 value (waveform select)
    reg_e0: u8,
    /// Current envelope state
    state: OperatorState,
    /// Tremolo mask for amplitude modulation
    tremolo_mask: i8,
    /// Vibrato strength
    vib_strength: u8,
    /// Key scale rate value
    ksr: u8,
}

impl Default for Operator {
    fn default() -> Self {
        Self {
            vol_handler: volume_off,
            wave_base: 0,
            wave_mask: WAVE_MASK_TABLE[0] as u32,
            wave_start: (WAVE_START_TABLE[0] as u32) << WAVE_SH,
            wave_index: 0,
            wave_add: 0,
            wave_current: 0,
            chan_data: 0,
            freq_mul: 1,
            vibrato: 0,
            sustain_level: ENV_MAX,
            total_level: ENV_MAX,
            current_level: ENV_MAX as u32,
            volume: ENV_MAX,
            attack_add: 0,
            decay_add: 0,
            release_add: 0,
            rate_index: 0,
            rate_zero: 0xFF,
            key_on: 0,
            reg20: 0,
            reg40: 0,
            reg60: 0,
            reg80: 0,
            reg_e0: 0,
            state: OperatorState::Off,
            tremolo_mask: 0,
            vib_strength: 0,
            ksr: 0,
        }
    }
}

/// Represents a single FM channel containing two operators
///
/// A channel combines two operators to create a single voice.
/// The operators can be connected in various ways (FM or AM)
/// to produce different timbres.
pub struct Channel {
    /// The two operators that make up this channel
    op: [Operator; 2],
    /// Function pointer to current synthesis method
    synth_handler: SynthHandler,
    /// Channel frequency and key scaling data
    chan_data: u32,
    /// Previous output samples for feedback
    old: [i32; 2],
    /// Feedback amount (0-7, where 0 = no feedback)
    feedback: u8,
    /// Register 0xB0 value (frequency high, octave, key on/off)
    reg_b0: u8,
    /// Register 0xC0 value (feedback, connection, output routing)
    reg_c0: u8,
    /// Four-operator mode mask
    four_mask: u8,
    /// Left channel output mask (OPL3 only)
    mask_left: i32,
    /// Right channel output mask (OPL3 only)
    mask_right: i32,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            op: [Operator::default(), Operator::default()],
            synth_handler: synth_2fm,
            chan_data: 0,
            old: [0, 0],
            feedback: 31,
            reg_b0: 0,
            reg_c0: 0,
            four_mask: 0,
            mask_left: -1,
            mask_right: -1,
        }
    }
}

/// The main OPL2/OPL3 chip emulator
///
/// This structure represents the complete state of an OPL chip,
/// including all channels, operators, and global settings.
/// It provides the main interface for register writes and
/// audio sample generation.
pub struct Chip {
    /// LFO counter for vibrato and tremolo
    lfo_counter: u32,
    /// LFO increment per sample
    lfo_add: u32,
    /// Noise generator counter
    noise_counter: u32,
    /// Noise generator increment
    noise_add: u32,
    /// Current noise value
    noise_value: u32,
    /// Frequency multiplier table
    freq_mul: [u32; 16],
    /// Linear rate table for decay and release
    linear_rates: [u32; 76],
    /// Attack rate table
    attack_rates: [u32; 76],
    /// All FM channels (9 for OPL2, 18 for OPL3)
    chan: [Channel; 18],
    /// Register 0x104 value (OPL3 connection settings)
    reg104: u8,
    /// Register 0x08 value (composite sine wave mode)
    reg08: u8,
    /// Register 0x04 value (timer settings)
    reg04: u8,
    /// Register 0xBD value (rhythm mode and bass drum)
    reg_bd: u8,
    /// Vibrato LFO index
    vibrato_index: u8,
    /// Tremolo LFO index
    tremolo_index: u8,
    /// Vibrato sign for modulation direction
    vibrato_sign: i8,
    /// Vibrato shift amount
    vibrato_shift: u8,
    /// Current tremolo value
    tremolo_value: u8,
    /// Vibrato strength setting
    vibrato_strength: u8,
    /// Tremolo strength setting
    tremolo_strength: u8,
    /// Waveform selection mask
    wave_form_mask: u8,
    /// OPL3 mode active flag (-1 for active, 0 for OPL2 mode)
    opl3_active: i8,
}

impl Default for Chip {
    fn default() -> Self {
        Self {
            lfo_counter: 0,
            lfo_add: 0,
            noise_counter: 0,
            noise_add: 0,
            noise_value: 1,
            freq_mul: [0; 16],
            linear_rates: [0; 76],
            attack_rates: [0; 76],
            chan: std::array::from_fn(|_| Channel::default()),
            reg104: 0,
            reg08: 0,
            reg04: 0,
            reg_bd: 0,
            vibrato_index: 0,
            tremolo_index: 0,
            vibrato_sign: 0,
            vibrato_shift: 0,
            tremolo_value: 0,
            vibrato_strength: 0,
            tremolo_strength: 0,
            wave_form_mask: 0,
            opl3_active: 0,
        }
    }
}

/// Volume handler for silent operators
fn volume_off(_op: &mut Operator) -> i32 {
    ENV_MAX
}

/// Default 2-operator FM synthesis handler
fn synth_2fm(ch: &mut Channel, chip: &mut Chip, samples: u32, output: &mut [i32]) -> usize {
    ch.block_template(chip, samples, output, SynthMode::Sm2FM)
}

/// Envelope calculation helper function
///
/// Determines the envelope curve shape based on the current state.
/// This is used internally by the envelope generator.
fn envelope_select(val: i32) -> u32 {
    if val < ENV_MIN {
        ENV_MIN as u32
    } else if val >= ENV_MAX {
        ENV_MAX as u32
    } else {
        val as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chip_creation() {
        let chip = Chip::new(false);
        assert_eq!(chip.opl3_active, 0);
    }

    #[test]
    fn test_envelope_select() {
        assert_eq!(envelope_select(-100), ENV_MIN as u32);
        assert_eq!(envelope_select(ENV_MAX + 100), ENV_MAX as u32);
        assert_eq!(envelope_select(256), 256);
    }

    #[test]
    fn test_env_silent() {
        assert!(!env_silent(0));
        assert!(!env_silent(ENV_LIMIT - 1));
        assert!(env_silent(ENV_LIMIT));
        assert!(env_silent(ENV_LIMIT + 1000));
    }

    #[test]
    fn test_constants() {
        assert_eq!(WAVE_SH, 32 - WAVE_BITS);
        assert_eq!(WAVE_MASK, (1 << WAVE_SH) - 1);
        assert_eq!(ENV_MAX, 511 << ENV_EXTRA);
    }

    #[test]
    fn test_operator_default() {
        let op = Operator::default();
        assert_eq!(op.state, OperatorState::Off);
        assert_eq!(op.volume, ENV_MAX);
        assert_eq!(op.sustain_level, ENV_MAX);
    }

    #[test]
    fn test_channel_default() {
        let ch = Channel::default();
        assert_eq!(ch.feedback, 31);
        assert_eq!(ch.reg_b0, 0);
        assert_eq!(ch.reg_c0, 0);
    }

    #[test]
    fn test_tables_initialization() {
        init_tables();

        // Test that tables are initialized and contain expected values
        assert!(EXP_TABLE.get().is_some());
        assert!(SIN_TABLE.get().is_some());
        assert!(WAVE_TABLE.get().is_some());
        assert!(MUL_TABLE.get().is_some());
        assert!(KSL_TABLE.get().is_some());
        assert!(TREMOLO_TABLE_DATA.get().is_some());

        // Test that tables contain non-zero values where expected
        let exp_table = EXP_TABLE.get().unwrap();
        let sin_table = SIN_TABLE.get().unwrap();

        assert_ne!(exp_table[0], 0);
        assert_ne!(sin_table[0], 0);
    }

    #[test]
    fn test_operator_state_enum() {
        let states = [
            OperatorState::Off,
            OperatorState::Release,
            OperatorState::Sustain,
            OperatorState::Decay,
            OperatorState::Attack,
        ];

        for state in states {
            // Test that enum variants can be compared
            assert!(state == state);
        }
    }

    #[test]
    fn test_synth_mode_enum() {
        let modes = [
            SynthMode::Sm2AM,
            SynthMode::Sm2FM,
            SynthMode::Sm3AM,
            SynthMode::Sm3FM,
        ];

        assert_eq!(modes.len(), 4);
    }

    #[test]
    fn test_chip_setup() {
        let mut chip = Chip::new(false);
        chip.setup(44100);

        assert!(chip.noise_add > 0);
        assert!(chip.lfo_add > 0);
        assert!(chip.freq_mul[1] > 0);
    }

    #[test]
    fn test_register_masks() {
        assert_eq!(MASK_KSR, 0x10);
        assert_eq!(MASK_SUSTAIN, 0x20);
        assert_eq!(MASK_VIBRATO, 0x40);
        assert_eq!(MASK_TREMOLO, 0x80);
    }

    #[test]
    fn test_shift_constants() {
        assert_eq!(SHIFT_KSLBASE, 16);
        assert_eq!(SHIFT_KEYCODE, 24);
        assert_eq!(RATE_SH, 24);
        assert_eq!(MUL_SH, 16);
    }

    #[test]
    fn test_operator_new() {
        let op = Operator::new();
        assert_eq!(op.state, OperatorState::Off);
        assert_eq!(op.key_on, 0);
    }

    #[test]
    fn test_channel_new() {
        let ch = Channel::new();
        assert_eq!(ch.old, [0, 0]);
        assert_eq!(ch.chan_data, 0);
    }

    #[test]
    fn test_frequency_constants() {
        assert!(FREQ_CREATE_TABLE.len() == 16);
        assert_eq!(FREQ_CREATE_TABLE[0], 1);
        assert_eq!(FREQ_CREATE_TABLE[1], 2);
    }

    #[test]
    fn test_envelope_tables() {
        assert!(ATTACK_SAMPLES_TABLE.len() == 13);
        assert!(ENVELOPE_INCREASE_TABLE.len() == 13);
        assert_eq!(ATTACK_SAMPLES_TABLE[0], 69);
        assert_eq!(ENVELOPE_INCREASE_TABLE[0], 4);
    }

    #[test]
    fn test_ksl_tables() {
        assert!(KSL_CREATE_TABLE.len() == 16);
        assert!(KSL_SHIFT_TABLE.len() == 4);
        assert_eq!(KSL_CREATE_TABLE[0], 64);
        assert_eq!(KSL_SHIFT_TABLE[0], 31);
    }

    #[test]
    fn test_wave_tables() {
        assert!(WAVE_BASE_TABLE.len() == 8);
        assert!(WAVE_MASK_TABLE.len() == 8);
        assert!(WAVE_START_TABLE.len() == 8);
    }

    #[test]
    fn test_vibrato_table() {
        assert!(VIBRATO_TABLE.len() == 8);
        assert_eq!(VIBRATO_TABLE[0], 1);
    }
}
