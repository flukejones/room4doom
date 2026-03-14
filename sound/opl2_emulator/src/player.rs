//! OPL MIDI player state machine.
//!
//! Drives an OPL2 `Chip` with MIDI events parsed from standard MIDI files.
//! Backend-agnostic — no SDL2 or rodio dependency. Consumers wrap this in
//! their audio callback / `Source` implementation.

use crate::{Chip, init_tables};
use wad::WadData;

const GENMIDI_HEADER: &str = "#OPL_II#";
const GENMIDI_NUM_INSTRS: usize = 128;
const GENMIDI_NUM_PERCUSSION: usize = 47;
const GENMIDI_FLAG_FIXED: u16 = 0x0001;
const GENMIDI_FLAG_2VOICE: u16 = 0x0004;

const OPL_CHANNELS: usize = 9;

/// GZDoom normalizes OPL output by dividing by 10240.0 for float [-1,1].
/// For i16 output: 32767.0 / 10240.0 ≈ 3.2.
const OPL_OUTPUT_SCALE: f32 = 32767.0 / 10240.0;

const VOLUME_MAPPING_TABLE: [u8; 128] = [
    0, 1, 3, 5, 6, 8, 10, 11, 13, 14, 16, 17, 19, 20, 22, 23, 25, 26, 27, 29, 30, 32, 33, 34, 36,
    37, 39, 41, 43, 45, 47, 49, 50, 52, 54, 55, 57, 59, 60, 61, 63, 64, 66, 67, 68, 69, 71, 72, 73,
    74, 75, 76, 77, 79, 80, 81, 82, 83, 84, 84, 85, 86, 87, 88, 89, 90, 91, 92, 92, 93, 94, 95, 96,
    96, 97, 98, 99, 99, 100, 101, 101, 102, 103, 103, 104, 105, 105, 106, 107, 107, 108, 109, 109,
    110, 110, 111, 112, 112, 113, 113, 114, 114, 115, 115, 116, 117, 117, 118, 118, 119, 119, 120,
    120, 121, 121, 122, 122, 123, 123, 123, 124, 124, 125, 125, 126, 126, 127, 127,
];

#[rustfmt::skip]
const FREQUENCY_CURVE: [u16; 668] = [
    0x133, 0x133, 0x134, 0x134, 0x135, 0x136, 0x136, 0x137, 0x137, 0x138, 0x138, 0x139, 0x139,
    0x13A, 0x13B, 0x13B, 0x13C, 0x13C, 0x13D, 0x13D, 0x13E, 0x13F, 0x13F, 0x140, 0x140, 0x141,
    0x142, 0x142, 0x143, 0x143, 0x144, 0x144, 0x145, 0x146, 0x146, 0x147, 0x147, 0x148, 0x149,
    0x149, 0x14A, 0x14A, 0x14B, 0x14C, 0x14C, 0x14D, 0x14D, 0x14E, 0x14F, 0x14F, 0x150, 0x150,
    0x151, 0x152, 0x152, 0x153, 0x153, 0x154, 0x155, 0x155, 0x156, 0x157, 0x157, 0x158, 0x158,
    0x159, 0x15A, 0x15A, 0x15B, 0x15B, 0x15C, 0x15D, 0x15D, 0x15E, 0x15F, 0x15F, 0x160, 0x161,
    0x161, 0x162, 0x162, 0x163, 0x164, 0x164, 0x165, 0x166, 0x166, 0x167, 0x168, 0x168, 0x169,
    0x16A, 0x16A, 0x16B, 0x16C, 0x16C, 0x16D, 0x16E, 0x16E, 0x16F, 0x170, 0x170, 0x171, 0x172,
    0x172, 0x173, 0x174, 0x174, 0x175, 0x176, 0x176, 0x177, 0x178, 0x178, 0x179, 0x17A, 0x17A,
    0x17B, 0x17C, 0x17C, 0x17D, 0x17E, 0x17E, 0x17F, 0x180, 0x181, 0x181, 0x182, 0x183, 0x183,
    0x184, 0x185, 0x185, 0x186, 0x187, 0x188, 0x188, 0x189, 0x18A, 0x18A, 0x18B, 0x18C, 0x18D,
    0x18D, 0x18E, 0x18F, 0x18F, 0x190, 0x191, 0x192, 0x192, 0x193, 0x194, 0x194, 0x195, 0x196,
    0x197, 0x197, 0x198, 0x199, 0x19A, 0x19A, 0x19B, 0x19C, 0x19D, 0x19D, 0x19E, 0x19F, 0x1A0,
    0x1A0, 0x1A1, 0x1A2, 0x1A3, 0x1A3, 0x1A4, 0x1A5, 0x1A6, 0x1A6, 0x1A7, 0x1A8, 0x1A9, 0x1A9,
    0x1AA, 0x1AB, 0x1AC, 0x1AD, 0x1AD, 0x1AE, 0x1AF, 0x1B0, 0x1B0, 0x1B1, 0x1B2, 0x1B3, 0x1B4,
    0x1B4, 0x1B5, 0x1B6, 0x1B7, 0x1B8, 0x1B8, 0x1B9, 0x1BA, 0x1BB, 0x1BC, 0x1BC, 0x1BD, 0x1BE,
    0x1BF, 0x1C0, 0x1C0, 0x1C1, 0x1C2, 0x1C3, 0x1C4, 0x1C4, 0x1C5, 0x1C6, 0x1C7, 0x1C8, 0x1C9,
    0x1C9, 0x1CA, 0x1CB, 0x1CC, 0x1CD, 0x1CE, 0x1CE, 0x1CF, 0x1D0, 0x1D1, 0x1D2, 0x1D3, 0x1D3,
    0x1D4, 0x1D5, 0x1D6, 0x1D7, 0x1D8, 0x1D8, 0x1D9, 0x1DA, 0x1DB, 0x1DC, 0x1DD, 0x1DE, 0x1DE,
    0x1DF, 0x1E0, 0x1E1, 0x1E2, 0x1E3, 0x1E4, 0x1E5, 0x1E5, 0x1E6, 0x1E7, 0x1E8, 0x1E9, 0x1EA,
    0x1EB, 0x1EC, 0x1ED, 0x1ED, 0x1EE, 0x1EF, 0x1F0, 0x1F1, 0x1F2, 0x1F3, 0x1F4, 0x1F5, 0x1F6,
    0x1F6, 0x1F7, 0x1F8, 0x1F9, 0x1FA, 0x1FB, 0x1FC, 0x1FD, 0x1FE, 0x1FF, 0x200, 0x201, 0x201,
    0x202, 0x203, 0x204, 0x205, 0x206, 0x207, 0x208, 0x209, 0x20A, 0x20B, 0x20C, 0x20D, 0x20E,
    0x20F, 0x210, 0x210, 0x211, 0x212, 0x213, 0x214, 0x215, 0x216, 0x217, 0x218, 0x219, 0x21A,
    0x21B, 0x21C, 0x21D, 0x21E, 0x21F, 0x220, 0x221, 0x222, 0x223, 0x224, 0x225, 0x226, 0x227,
    0x228, 0x229, 0x22A, 0x22B, 0x22C, 0x22D, 0x22E, 0x22F, 0x230, 0x231, 0x232, 0x233, 0x234,
    0x235, 0x236, 0x237, 0x238, 0x239, 0x23A, 0x23B, 0x23C, 0x23D, 0x23E, 0x23F, 0x240, 0x241,
    0x242, 0x244, 0x245, 0x246, 0x247, 0x248, 0x249, 0x24A, 0x24B, 0x24C, 0x24D, 0x24E, 0x24F,
    0x250, 0x251, 0x252, 0x253, 0x254, 0x256, 0x257, 0x258, 0x259, 0x25A, 0x25B, 0x25C, 0x25D,
    0x25E, 0x25F, 0x260, 0x262, 0x263, 0x264, 0x265, 0x266, 0x267, 0x268, 0x269, 0x26A, 0x26C,
    0x26D, 0x26E, 0x26F, 0x270, 0x271, 0x272, 0x273, 0x275, 0x276, 0x277, 0x278, 0x279, 0x27A,
    0x27B, 0x27D, 0x27E, 0x27F, 0x280, 0x281, 0x282, 0x284, 0x285, 0x286, 0x287, 0x288, 0x289,
    0x28B, 0x28C, 0x28D, 0x28E, 0x28F, 0x290, 0x292, 0x293, 0x294, 0x295, 0x296, 0x298, 0x299,
    0x29A, 0x29B, 0x29C, 0x29E, 0x29F, 0x2A0, 0x2A1, 0x2A2, 0x2A4, 0x2A5, 0x2A6, 0x2A7, 0x2A9,
    0x2AA, 0x2AB, 0x2AC, 0x2AE, 0x2AF, 0x2B0, 0x2B1, 0x2B2, 0x2B4, 0x2B5, 0x2B6, 0x2B7, 0x2B9,
    0x2BA, 0x2BB, 0x2BD, 0x2BE, 0x2BF, 0x2C0, 0x2C2, 0x2C3, 0x2C4, 0x2C5, 0x2C7, 0x2C8, 0x2C9,
    0x2CB, 0x2CC, 0x2CD, 0x2CE, 0x2D0, 0x2D1, 0x2D2, 0x2D4, 0x2D5, 0x2D6, 0x2D8, 0x2D9, 0x2DA,
    0x2DC, 0x2DD, 0x2DE, 0x2E0, 0x2E1, 0x2E2, 0x2E4, 0x2E5, 0x2E6, 0x2E8, 0x2E9, 0x2EA, 0x2EC,
    0x2ED, 0x2EE, 0x2F0, 0x2F1, 0x2F2, 0x2F4, 0x2F5, 0x2F6, 0x2F8, 0x2F9, 0x2FB, 0x2FC, 0x2FD,
    0x2FF, 0x300, 0x302, 0x303, 0x304, 0x306, 0x307, 0x309, 0x30A, 0x30B, 0x30D, 0x30E, 0x310,
    0x311, 0x312, 0x314, 0x315, 0x317, 0x318, 0x31A, 0x31B, 0x31C, 0x31E, 0x31F, 0x321, 0x322,
    0x324, 0x325, 0x327, 0x328, 0x329, 0x32B, 0x32C, 0x32E, 0x32F, 0x331, 0x332, 0x334, 0x335,
    0x337, 0x338, 0x33A, 0x33B, 0x33D, 0x33E, 0x340, 0x341, 0x343, 0x344, 0x346, 0x347, 0x349,
    0x34A, 0x34C, 0x34D, 0x34F, 0x350, 0x352, 0x353, 0x355, 0x357, 0x358, 0x35A, 0x35B, 0x35D,
    0x35E, 0x360, 0x361, 0x363, 0x365, 0x366, 0x368, 0x369, 0x36B, 0x36C, 0x36E, 0x370, 0x371,
    0x373, 0x374, 0x376, 0x378, 0x379, 0x37B, 0x37C, 0x37E, 0x380, 0x381, 0x383, 0x384, 0x386,
    0x388, 0x389, 0x38B, 0x38D, 0x38E, 0x390, 0x392, 0x393, 0x395, 0x397, 0x398, 0x39A, 0x39C,
    0x39D, 0x39F, 0x3A1, 0x3A2, 0x3A4, 0x3A6, 0x3A7, 0x3A9, 0x3AB, 0x3AC, 0x3AE, 0x3B0, 0x3B1,
    0x3B3, 0x3B5, 0x3B7, 0x3B8, 0x3BA, 0x3BC, 0x3BD, 0x3BF, 0x3C1, 0x3C3, 0x3C4, 0x3C6, 0x3C8,
    0x3CA, 0x3CB, 0x3CD, 0x3CF, 0x3D1, 0x3D2, 0x3D4, 0x3D6, 0x3D8, 0x3DA, 0x3DB, 0x3DD, 0x3DF,
    0x3E1, 0x3E3, 0x3E4, 0x3E6, 0x3E8, 0x3EA, 0x3EC, 0x3ED, 0x3EF, 0x3F1, 0x3F3, 0x3F5, 0x3F6,
    0x3F8, 0x3FA, 0x3FC, 0x3FE, 0x36C,
];

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct GenmidiOperator {
    tremolo: u8,
    attack: u8,
    sustain: u8,
    waveform: u8,
    scale: u8,
    level: u8,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct GenmidiVoice {
    modulator: GenmidiOperator,
    feedback: u8,
    carrier: GenmidiOperator,
    unused: u8,
    base_note_offset: i16,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct GenmidiInstrument {
    flags: u16,
    fine_tuning: u8,
    fixed_note: u8,
    voices: [GenmidiVoice; 2],
}

impl GenmidiInstrument {
    fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < 36 {
            return Err("Insufficient data for instrument".to_string());
        }

        let flags = u16::from_le_bytes([data[0], data[1]]);
        let fine_tuning = data[2];
        let fixed_note = data[3];

        let mut voices = [GenmidiVoice {
            modulator: GenmidiOperator {
                tremolo: 0,
                attack: 0,
                sustain: 0,
                waveform: 0,
                scale: 0,
                level: 0,
            },
            feedback: 0,
            carrier: GenmidiOperator {
                tremolo: 0,
                attack: 0,
                sustain: 0,
                waveform: 0,
                scale: 0,
                level: 0,
            },
            unused: 0,
            base_note_offset: 0,
        }; 2];

        let mut offset = 4;
        for voice in voices.iter_mut() {
            voice.modulator.tremolo = data[offset];
            voice.modulator.attack = data[offset + 1];
            voice.modulator.sustain = data[offset + 2];
            voice.modulator.waveform = data[offset + 3];
            voice.modulator.scale = data[offset + 4];
            voice.modulator.level = data[offset + 5];
            voice.feedback = data[offset + 6];
            voice.carrier.tremolo = data[offset + 7];
            voice.carrier.attack = data[offset + 8];
            voice.carrier.sustain = data[offset + 9];
            voice.carrier.waveform = data[offset + 10];
            voice.carrier.scale = data[offset + 11];
            voice.carrier.level = data[offset + 12];
            voice.unused = data[offset + 13];
            voice.base_note_offset = i16::from_le_bytes([data[offset + 14], data[offset + 15]]);
            offset += 16;
        }

        Ok(Self {
            flags,
            fine_tuning,
            fixed_note,
            voices,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct MidiEvent {
    absolute_time: u32,
    event_type: u8,
    channel: u8,
    data1: u8,
    data2: u8,
    data3: u8,
}

#[derive(Debug, Clone)]
struct OplNote {
    midi_channel: u8,
    midi_note: u8,
    opl_channel: usize,
    note_volume: u8,
    current_instrument: GenmidiInstrument,
    instrument_voice: usize,
    sustained: bool,
    freq: u16,
    reg_volume: u8,
}

/// MIDI CC state per channel
#[derive(Debug, Clone, Copy)]
struct ChannelData {
    /// MIDI CC7 channel volume (0-127).
    volume: u8,
    /// MIDI CC11 expression (0-127).
    expression: u8,
    /// Current program/instrument number.
    program: u8,
    /// Whether sustain pedal is active.
    sustain: bool,
    /// Pitch bend offset (-64..+63).
    bend: i32,
}

impl Default for ChannelData {
    fn default() -> Self {
        Self {
            volume: 127,
            expression: 127,
            program: 0,
            sustain: false,
            bend: 0,
        }
    }
}

struct MidiTrack {
    events: Vec<MidiEvent>,
    position: usize,
    tick_time: u32,
}

impl MidiTrack {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            position: 0,
            tick_time: 0,
        }
    }

    fn reset(&mut self) {
        self.position = 0;
        self.tick_time = 0;
    }
}

/// Backend-agnostic OPL MIDI player.
///
/// Manages MIDI parsing, note allocation across 9 OPL channels, GENMIDI
/// instrument loading, and sample generation. Wrap in an audio callback
/// or `rodio::Source` to produce output.
pub struct OplPlayerState {
    chip: Chip,
    midi_track: MidiTrack,
    playing_notes: Vec<OplNote>,
    opl_channels: [bool; OPL_CHANNELS],
    sample_rate: u32,
    is_playing: bool,
    loop_music: bool,
    tempo: u32,
    ticks_per_quarter: u32,
    samples_per_tick: f64,
    sample_counter: f64,
    main_instruments: Vec<GenmidiInstrument>,
    percussion_instruments: Vec<GenmidiInstrument>,
    channels: [ChannelData; 16],
    /// Master volume (0-128)
    pub volume: i32,
}

impl OplPlayerState {
    /// Create a new player, loading GENMIDI from the WAD.
    pub fn new(sample_rate: u32, wad: &WadData) -> Self {
        init_tables();
        let mut chip = Chip::new();
        chip.setup(sample_rate);

        // Initialize all registers like Chocolate Doom's OPL_InitRegisters
        for r in 0x40..=0x55 {
            chip.write_reg(r, 0x3F);
        }
        for r in 0x60..=0xF5 {
            chip.write_reg(r, 0x00);
        }
        for r in 1..0x40 {
            chip.write_reg(r, 0x00);
        }
        chip.write_reg(0x01, 0x20);

        let (main_instruments, percussion_instruments) = Self::load_genmidi(wad);

        Self {
            chip,
            midi_track: MidiTrack::new(),
            playing_notes: Vec::new(),
            opl_channels: [false; OPL_CHANNELS],
            sample_rate,
            is_playing: false,
            loop_music: false,
            tempo: 500000,
            ticks_per_quarter: 96,
            samples_per_tick: 0.0,
            sample_counter: 0.0,
            main_instruments,
            percussion_instruments,
            channels: [ChannelData::default(); 16],
            volume: 128,
        }
    }

    /// Reinitialize the chip for a different sample rate.
    pub fn reinit_rate(&mut self, rate: u32) {
        self.chip.setup(rate);
        self.sample_rate = rate;
        self.calculate_timing();
    }

    /// Whether music is currently playing.
    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    /// Load MIDI data for playback.
    pub fn load_music(&mut self, midi_data: Vec<u8>) -> Result<(), String> {
        self.midi_track = MidiTrack::new();
        self.parse_midi(&midi_data)?;
        self.calculate_timing();
        Ok(())
    }

    /// Start playback from the beginning.
    pub fn start_playback(&mut self, looping: bool) {
        self.is_playing = true;
        self.loop_music = looping;
        self.midi_track.reset();
        self.sample_counter = 0.0;
        self.stop_all_notes();
        self.channels = [ChannelData::default(); 16];
    }

    /// Stop playback and silence all notes.
    pub fn stop_playback(&mut self) {
        self.is_playing = false;
        self.stop_all_notes();
    }

    /// Generate mono i16 samples into the buffer.
    pub fn generate_samples(&mut self, buffer: &mut [i16]) {
        if !self.is_playing {
            buffer.fill(0);
            return;
        }

        for sample in buffer.iter_mut() {
            self.sample_counter += 1.0;
            while self.sample_counter >= self.samples_per_tick {
                self.sample_counter -= self.samples_per_tick;
                self.midi_track.tick_time += 1;
                self.process_midi_events();
            }

            let mut opl_sample = [0i32; 1];
            self.chip.generate_block_2(1, &mut opl_sample);

            let scaled = opl_sample[0] as f32 * OPL_OUTPUT_SCALE * (self.volume as f32 / 128.0);
            *sample = scaled.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        }
    }

    /// Refresh OPL volume registers for all voices on every channel.
    /// Call after changing `self.volume`.
    pub fn refresh_all_volumes(&mut self) {
        let updates: Vec<_> = self
            .playing_notes
            .iter()
            .map(|n| {
                let ch = self.channels[n.midi_channel as usize];
                (
                    n.opl_channel,
                    n.note_volume,
                    ch.volume,
                    ch.expression,
                    n.current_instrument,
                    n.instrument_voice,
                )
            })
            .collect();

        for (opl_channel, note_volume, channel_volume, expression, instrument, voice_num) in updates
        {
            let reg_volume = self.set_voice_volume(
                opl_channel,
                note_volume,
                channel_volume,
                expression,
                &instrument,
                voice_num,
            );

            for note in &mut self.playing_notes {
                if note.opl_channel == opl_channel {
                    note.reg_volume = reg_volume;
                    break;
                }
            }
        }
    }

    // ── Private ──────────────────────────────────────────────────────────

    fn load_genmidi(wad: &WadData) -> (Vec<GenmidiInstrument>, Vec<GenmidiInstrument>) {
        if let Some(lump) = wad.get_lump("GENMIDI")
            && lump.data.len() >= GENMIDI_HEADER.len() + 8
        {
            let header = &lump.data[..GENMIDI_HEADER.len()];
            if header == GENMIDI_HEADER.as_bytes()
                && lump.data[GENMIDI_HEADER.len()..].len()
                    >= GENMIDI_NUM_INSTRS * 36 + GENMIDI_NUM_PERCUSSION * 36
            {
                let data_start = GENMIDI_HEADER.len();
                let main_data = &lump.data[data_start..];

                let mut main_instrs = Vec::with_capacity(GENMIDI_NUM_INSTRS);
                let mut perc_instrs = Vec::with_capacity(GENMIDI_NUM_PERCUSSION);

                for i in 0..GENMIDI_NUM_INSTRS {
                    let offset = i * 36;
                    if let Ok(instr) =
                        GenmidiInstrument::from_bytes(&main_data[offset..offset + 36])
                    {
                        main_instrs.push(instr);
                    } else {
                        main_instrs.push(Self::default_instrument());
                    }
                }

                let perc_start = GENMIDI_NUM_INSTRS * 36;
                for i in 0..GENMIDI_NUM_PERCUSSION {
                    let offset = perc_start + i * 36;
                    if let Ok(instr) =
                        GenmidiInstrument::from_bytes(&main_data[offset..offset + 36])
                    {
                        perc_instrs.push(instr);
                    } else {
                        perc_instrs.push(Self::default_instrument());
                    }
                }

                return (main_instrs, perc_instrs);
            }
        }

        (
            vec![Self::default_instrument(); GENMIDI_NUM_INSTRS],
            vec![Self::default_instrument(); GENMIDI_NUM_PERCUSSION],
        )
    }

    fn default_instrument() -> GenmidiInstrument {
        GenmidiInstrument {
            flags: 0,
            fine_tuning: 128,
            fixed_note: 0,
            voices: [
                GenmidiVoice {
                    modulator: GenmidiOperator {
                        tremolo: 0x00,
                        attack: 0xF0,
                        sustain: 0xF0,
                        waveform: 0x00,
                        scale: 0x00,
                        level: 0x00,
                    },
                    feedback: 0x0E,
                    carrier: GenmidiOperator {
                        tremolo: 0x00,
                        attack: 0xF0,
                        sustain: 0xF0,
                        waveform: 0x00,
                        scale: 0x00,
                        level: 0x00,
                    },
                    unused: 0,
                    base_note_offset: 0,
                },
                GenmidiVoice {
                    modulator: GenmidiOperator {
                        tremolo: 0x00,
                        attack: 0xF0,
                        sustain: 0xF0,
                        waveform: 0x00,
                        scale: 0x00,
                        level: 0x00,
                    },
                    feedback: 0x0E,
                    carrier: GenmidiOperator {
                        tremolo: 0x00,
                        attack: 0xF0,
                        sustain: 0xF0,
                        waveform: 0x00,
                        scale: 0x00,
                        level: 0x00,
                    },
                    unused: 0,
                    base_note_offset: 0,
                },
            ],
        }
    }

    fn calculate_timing(&mut self) {
        let seconds_per_tick = (self.tempo as f64 / 1_000_000.0) / self.ticks_per_quarter as f64;
        self.samples_per_tick = seconds_per_tick * self.sample_rate as f64;
    }

    fn parse_midi(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() < 14 {
            return Err("Invalid MIDI data".to_string());
        }

        let mut pos = 0;

        if data[0..4] != *b"MThd" {
            return Err("Not a valid MIDI file".to_string());
        }

        pos += 4;
        let header_length =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;

        if header_length < 6 {
            return Err("Invalid MIDI header".to_string());
        }

        let _format = u16::from_be_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        let num_tracks = u16::from_be_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        self.ticks_per_quarter = u16::from_be_bytes([data[pos], data[pos + 1]]) as u32;
        pos += 2;

        pos += (header_length - 6) as usize;

        for _ in 0..num_tracks {
            if pos + 8 > data.len() || &data[pos..pos + 4] != b"MTrk" {
                break;
            }

            pos += 4;
            let track_length =
                u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                    as usize;
            pos += 4;

            let track_end = pos + track_length;
            let mut running_status = 0u8;
            let mut absolute_time = 0u32;

            while pos < track_end {
                let (delta_time, new_pos) = self.parse_variable_length(data, pos)?;
                pos = new_pos;
                absolute_time += delta_time;

                if pos >= track_end {
                    break;
                }

                let mut status = data[pos];
                if status < 0x80 {
                    status = running_status;
                } else {
                    pos += 1;
                    running_status = status;
                }

                let event_type = status & 0xF0;
                let channel = status & 0x0F;

                match event_type {
                    0x80 | 0x90 | 0xB0 | 0xC0 | 0xE0 => {
                        let data1 = if pos < track_end { data[pos] } else { 0 };
                        pos += 1;
                        let data2 = if event_type != 0xC0 && pos < track_end {
                            data[pos]
                        } else {
                            0
                        };
                        if event_type != 0xC0 {
                            pos += 1;
                        }

                        self.midi_track.events.push(MidiEvent {
                            absolute_time,
                            event_type,
                            channel,
                            data1,
                            data2,
                            data3: 0,
                        });
                    }
                    0xFF => {
                        if pos >= track_end {
                            break;
                        }
                        let meta_type = data[pos];
                        pos += 1;

                        let (length, new_pos) = self.parse_variable_length(data, pos)?;
                        pos = new_pos;

                        if meta_type == 0x51 && length >= 3 && pos + 3 <= track_end {
                            self.midi_track.events.push(MidiEvent {
                                absolute_time,
                                event_type: 0xFF,
                                channel: meta_type,
                                data1: data[pos],
                                data2: data[pos + 1],
                                data3: data[pos + 2],
                            });
                        }

                        pos += length as usize;
                    }
                    _ => {
                        pos += 1;
                    }
                }
            }

            pos = track_end;
        }

        self.midi_track.events.sort_by_key(|e| e.absolute_time);
        self.calculate_timing();
        Ok(())
    }

    fn parse_variable_length(&self, data: &[u8], mut pos: usize) -> Result<(u32, usize), String> {
        let mut value = 0u32;
        let mut byte;

        loop {
            if pos >= data.len() {
                return Err("Unexpected end of data".to_string());
            }
            byte = data[pos];
            pos += 1;
            value = (value << 7) | (byte as u32 & 0x7F);
            if byte & 0x80 == 0 {
                break;
            }
        }

        Ok((value, pos))
    }

    fn stop_all_notes(&mut self) {
        for i in 0..OPL_CHANNELS {
            self.chip.write_reg(0xB0 + i as u32, 0);
        }
        self.playing_notes.clear();
        self.opl_channels.fill(false);
    }

    fn allocate_opl_channel(&mut self) -> Option<usize> {
        for (i, &used) in self.opl_channels.iter().enumerate() {
            if !used {
                self.opl_channels[i] = true;
                return Some(i);
            }
        }
        None
    }

    fn steal_voice(&mut self, new_channel: u8) -> Option<usize> {
        let mut best_idx = None;
        let mut best_priority = i32::MAX;

        for (idx, note) in self.playing_notes.iter().enumerate() {
            if note.sustained {
                continue;
            }

            let priority = self.calculate_voice_priority(note, new_channel);
            if priority < best_priority {
                best_priority = priority;
                best_idx = Some(idx);
            }
        }

        if let Some(idx) = best_idx {
            let stolen_note = self.playing_notes.remove(idx);
            let opl_channel = stolen_note.opl_channel;

            self.chip
                .write_reg(0xB0 + opl_channel as u32, (stolen_note.freq >> 8) as u8);

            Some(opl_channel)
        } else {
            None
        }
    }

    fn calculate_voice_priority(&self, note: &OplNote, new_channel: u8) -> i32 {
        if note.instrument_voice != 0 {
            return i32::MIN;
        }

        if note.midi_channel > new_channel {
            return i32::MIN + 1;
        }

        i32::MAX
    }

    fn allocate_or_steal_channel(&mut self, channel: u8) -> Option<usize> {
        if let Some(opl_channel) = self.allocate_opl_channel() {
            Some(opl_channel)
        } else {
            self.steal_voice(channel)
        }
    }

    fn free_opl_channel(&mut self, channel: usize) {
        if channel < OPL_CHANNELS {
            self.opl_channels[channel] = false;
            self.chip.write_reg(0xB0 + channel as u32, 0);
        }
    }

    fn setup_instrument(
        &mut self,
        opl_channel: usize,
        instrument: &GenmidiInstrument,
        voice_num: usize,
    ) {
        let voice = &instrument.voices[voice_num];

        let op1_offset = if opl_channel < 3 {
            opl_channel
        } else if opl_channel < 6 {
            opl_channel + 5
        } else {
            opl_channel + 10
        };
        let op2_offset = op1_offset + 3;

        let modulating = (voice.feedback & 0x01) == 0;

        let carrier_level = (voice.carrier.scale & 0xC0) | 0x3F;
        self.chip.write_reg(0x40 + op2_offset as u32, carrier_level);
        self.chip
            .write_reg(0x20 + op2_offset as u32, voice.carrier.tremolo);
        self.chip
            .write_reg(0x60 + op2_offset as u32, voice.carrier.attack);
        self.chip
            .write_reg(0x80 + op2_offset as u32, voice.carrier.sustain);
        self.chip
            .write_reg(0xE0 + op2_offset as u32, voice.carrier.waveform);

        let modulator_level = if modulating {
            (voice.modulator.scale & 0xC0) | (voice.modulator.level & 0x3F)
        } else {
            (voice.modulator.scale & 0xC0) | 0x3F
        };
        self.chip
            .write_reg(0x40 + op1_offset as u32, modulator_level);
        self.chip
            .write_reg(0x20 + op1_offset as u32, voice.modulator.tremolo);
        self.chip
            .write_reg(0x60 + op1_offset as u32, voice.modulator.attack);
        self.chip
            .write_reg(0x80 + op1_offset as u32, voice.modulator.sustain);
        self.chip
            .write_reg(0xE0 + op1_offset as u32, voice.modulator.waveform);

        self.chip
            .write_reg(0xC0 + opl_channel as u32, voice.feedback | 0x30);
    }

    /// Compute and write operator volume registers for a voice.
    fn set_voice_volume(
        &mut self,
        opl_channel: usize,
        note_volume: u8,
        channel_volume: u8,
        expression: u8,
        instrument: &GenmidiInstrument,
        voice_num: usize,
    ) -> u8 {
        let voice = &instrument.voices[voice_num];

        let op1_offset = if opl_channel < 3 {
            opl_channel
        } else if opl_channel < 6 {
            opl_channel + 5
        } else {
            opl_channel + 10
        };
        let op2_offset = op1_offset + 3;

        let combined =
            (channel_volume as u32 * expression as u32 * note_volume as u32) / (127 * 127);
        let combined = combined.min(127) as usize;
        let full_volume = VOLUME_MAPPING_TABLE[combined] as u32;

        let car_level = (0x3F - (voice.carrier.level & 0x3F)) as u32;
        let car_reg = 0x3F - (car_level * full_volume / 128) as u8;
        let final_volume = car_reg | (voice.carrier.scale & 0xC0);

        self.chip.write_reg(0x40 + op2_offset as u32, final_volume);

        if (voice.feedback & 0x01) != 0 {
            let mod_level = (0x3F - (voice.modulator.level & 0x3F)) as u32;
            let mod_reg = 0x3F - (mod_level * full_volume / 128) as u8;
            let mod_final = mod_reg | (voice.modulator.scale & 0xC0);
            self.chip.write_reg(0x40 + op1_offset as u32, mod_final);
        }

        final_volume
    }

    fn refresh_channel_volumes(&mut self, midi_channel: u8) {
        let chan = self.channels[midi_channel as usize];
        let updates: Vec<_> = self
            .playing_notes
            .iter()
            .filter(|n| n.midi_channel == midi_channel)
            .map(|n| {
                (
                    n.opl_channel,
                    n.note_volume,
                    n.current_instrument,
                    n.instrument_voice,
                )
            })
            .collect();

        for (opl_channel, note_volume, instrument, voice_num) in updates {
            let reg_volume = self.set_voice_volume(
                opl_channel,
                note_volume,
                chan.volume,
                chan.expression,
                &instrument,
                voice_num,
            );

            for note in &mut self.playing_notes {
                if note.opl_channel == opl_channel {
                    note.reg_volume = reg_volume;
                    break;
                }
            }
        }
    }

    fn calculate_note_frequency(
        &self,
        note: u8,
        instrument: &GenmidiInstrument,
        voice_num: usize,
        channel: u8,
    ) -> (u16, u8) {
        let voice = &instrument.voices[voice_num];

        let actual_note = if (instrument.flags & GENMIDI_FLAG_FIXED) != 0 {
            instrument.fixed_note
        } else {
            let mut n = note as i32 + voice.base_note_offset as i32;
            while n < 0 {
                n += 12;
            }
            while n > 95 {
                n -= 12;
            }
            n as u8
        };

        let mut freq_index = 64 + (32 * actual_note as i32) + self.channels[channel as usize].bend;

        if voice_num != 0 {
            let fine_tune = (instrument.fine_tuning as i32 / 2) - 64;
            freq_index += fine_tune;
        }

        if freq_index < 0 {
            freq_index = 0;
        }

        let (freq, octave) = if freq_index < 284 {
            (FREQUENCY_CURVE[freq_index as usize], 0)
        } else {
            let sub_index = ((freq_index - 284) % (12 * 32)) as usize;
            let mut octave = ((freq_index - 284) / (12 * 32)) as u8;

            if octave >= 7 {
                octave = if sub_index < 5 { 7 } else { 6 };
            }

            (FREQUENCY_CURVE[sub_index + 284], octave)
        };

        (freq, octave)
    }

    fn update_voice_frequency(
        &mut self,
        opl_channel: usize,
        note: u8,
        channel: u8,
        instrument: &GenmidiInstrument,
        voice_num: usize,
    ) {
        let (freq, octave) = self.calculate_note_frequency(note, instrument, voice_num, channel);
        let freq_value = freq | ((octave as u16) << 10);

        for playing_note in &mut self.playing_notes {
            if playing_note.opl_channel == opl_channel {
                if playing_note.freq != freq_value {
                    self.chip
                        .write_reg(0xA0 + opl_channel as u32, (freq_value & 0xFF) as u8);
                    self.chip
                        .write_reg(0xB0 + opl_channel as u32, ((freq_value >> 8) | 0x20) as u8);
                    playing_note.freq = freq_value;
                }
                break;
            }
        }
    }

    fn key_on_voice(
        &mut self,
        channel: u8,
        note: u8,
        velocity: u8,
        instrument: &GenmidiInstrument,
        voice_num: usize,
    ) -> Option<(usize, u16, u8)> {
        let opl_channel = if voice_num == 0 {
            self.allocate_or_steal_channel(channel)
        } else {
            self.allocate_opl_channel()
        };

        if let Some(opl_channel) = opl_channel {
            self.setup_instrument(opl_channel, instrument, voice_num);

            let channel_data = self.channels[channel as usize];
            let reg_volume = self.set_voice_volume(
                opl_channel,
                velocity,
                channel_data.volume,
                channel_data.expression,
                instrument,
                voice_num,
            );

            let (freq, octave) =
                self.calculate_note_frequency(note, instrument, voice_num, channel);
            let freq_value = freq | ((octave as u16) << 10);

            self.chip
                .write_reg(0xA0 + opl_channel as u32, (freq_value & 0xFF) as u8);
            self.chip
                .write_reg(0xB0 + opl_channel as u32, ((freq_value >> 8) | 0x20) as u8);

            Some((opl_channel, freq_value, reg_volume))
        } else {
            None
        }
    }

    fn note_on(&mut self, channel: u8, note: u8, velocity: u8, program: u8) {
        let is_percussion = channel == 9;
        let instrument = if is_percussion && (35..=81).contains(&note) {
            let perc_idx = (note - 35) as usize;
            if perc_idx < self.percussion_instruments.len() {
                self.percussion_instruments[perc_idx]
            } else {
                return;
            }
        } else if (program as usize) < self.main_instruments.len() {
            self.main_instruments[program as usize]
        } else {
            return;
        };

        let is_double_voice = (instrument.flags & GENMIDI_FLAG_2VOICE) != 0;

        if let Some((opl_channel, freq, reg_volume)) =
            self.key_on_voice(channel, note, velocity, &instrument, 0)
        {
            self.playing_notes.push(OplNote {
                midi_channel: channel,
                midi_note: note,
                opl_channel,
                note_volume: velocity,
                current_instrument: instrument,
                instrument_voice: 0,
                sustained: false,
                freq,
                reg_volume,
            });

            if is_double_voice
                && let Some((opl_channel2, freq2, reg_volume2)) =
                    self.key_on_voice(channel, note, velocity, &instrument, 1)
            {
                self.playing_notes.push(OplNote {
                    midi_channel: channel,
                    midi_note: note,
                    opl_channel: opl_channel2,
                    note_volume: velocity,
                    current_instrument: instrument,
                    instrument_voice: 1,
                    sustained: false,
                    freq: freq2,
                    reg_volume: reg_volume2,
                });
            }
        }
    }

    fn note_off(&mut self, channel: u8, note: u8) {
        let sustain_active = self.channels[channel as usize].sustain;

        let mut i = 0;
        while i < self.playing_notes.len() {
            if self.playing_notes[i].midi_channel == channel
                && self.playing_notes[i].midi_note == note
            {
                if sustain_active {
                    self.playing_notes[i].sustained = true;
                    i += 1;
                } else {
                    let opl_note = self.playing_notes.remove(i);
                    let opl_channel = opl_note.opl_channel;

                    self.chip
                        .write_reg(0xB0 + opl_channel as u32, (opl_note.freq >> 8) as u8);

                    self.opl_channels[opl_channel] = false;
                }
            } else {
                i += 1;
            }
        }
    }

    fn release_sustained_notes(&mut self, channel: u8) {
        let mut i = 0;
        while i < self.playing_notes.len() {
            if self.playing_notes[i].midi_channel == channel && self.playing_notes[i].sustained {
                let opl_note = self.playing_notes.remove(i);
                let opl_channel = opl_note.opl_channel;

                self.chip
                    .write_reg(0xB0 + opl_channel as u32, (opl_note.freq >> 8) as u8);

                self.opl_channels[opl_channel] = false;
            } else {
                i += 1;
            }
        }
    }

    fn process_midi_events(&mut self) {
        while self.midi_track.position < self.midi_track.events.len() {
            let event = &self.midi_track.events[self.midi_track.position];

            if self.midi_track.tick_time < event.absolute_time {
                break;
            }

            if event.event_type == 0xFF && event.channel == 0x51 {
                self.tempo = u32::from_be_bytes([0, event.data1, event.data2, event.data3]);
                self.calculate_timing();
                self.midi_track.position += 1;
                continue;
            }

            match event.event_type {
                0x80 => {
                    self.note_off(event.channel, event.data1);
                }
                0x90 => {
                    if event.data2 > 0 {
                        let program = self.channels[event.channel as usize].program;
                        self.note_on(event.channel, event.data1, event.data2, program);
                    } else {
                        self.note_off(event.channel, event.data1);
                    }
                }
                0xB0 => {
                    let event_channel = event.channel;
                    match event.data1 {
                        7 => {
                            self.channels[event_channel as usize].volume = event.data2;
                            self.refresh_channel_volumes(event_channel);
                        }
                        11 => {
                            self.channels[event_channel as usize].expression = event.data2;
                            self.refresh_channel_volumes(event_channel);
                        }
                        64 => {
                            let sustain_on = event.data2 >= 64;
                            self.channels[event_channel as usize].sustain = sustain_on;

                            if !sustain_on {
                                self.release_sustained_notes(event_channel);
                            }
                        }
                        120 => {
                            let mut i = 0;
                            while i < self.playing_notes.len() {
                                if self.playing_notes[i].midi_channel == event_channel {
                                    let opl_note = self.playing_notes.remove(i);
                                    self.free_opl_channel(opl_note.opl_channel);
                                } else {
                                    i += 1;
                                }
                            }
                        }
                        123 => {
                            self.channels[event_channel as usize].sustain = false;
                            let mut i = 0;
                            while i < self.playing_notes.len() {
                                if self.playing_notes[i].midi_channel == event_channel {
                                    let opl_note = self.playing_notes.remove(i);
                                    self.free_opl_channel(opl_note.opl_channel);
                                } else {
                                    i += 1;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                0xC0 => {
                    self.channels[event.channel as usize].program = event.data1;
                }
                0xE0 => {
                    let event_channel = event.channel;
                    self.channels[event_channel as usize].bend = event.data2 as i32 - 64;

                    let updates: Vec<_> = self
                        .playing_notes
                        .iter()
                        .filter(|n| n.midi_channel == event_channel)
                        .map(|n| {
                            (
                                n.opl_channel,
                                n.midi_note,
                                n.current_instrument,
                                n.instrument_voice,
                            )
                        })
                        .collect();

                    for (opl_channel, midi_note, instrument, voice_num) in updates {
                        self.update_voice_frequency(
                            opl_channel,
                            midi_note,
                            event_channel,
                            &instrument,
                            voice_num,
                        );
                    }
                }
                _ => {}
            }

            self.midi_track.position += 1;
        }

        // Check if track finished
        if self.midi_track.position >= self.midi_track.events.len() {
            if self.loop_music {
                let mut i = 0;
                while i < self.playing_notes.len() {
                    let note = &self.playing_notes[i];
                    let is_percussion = note.midi_channel == 9;

                    if !is_percussion {
                        let opl_note = self.playing_notes.remove(i);
                        let opl_channel = opl_note.opl_channel;
                        self.chip
                            .write_reg(0xB0 + opl_channel as u32, (opl_note.freq >> 8) as u8);
                        self.opl_channels[opl_channel] = false;
                    } else {
                        i += 1;
                    }
                }

                self.midi_track.reset();
                self.channels = [ChannelData::default(); 16];
            } else {
                self.is_playing = false;
            }
        }
    }
}
