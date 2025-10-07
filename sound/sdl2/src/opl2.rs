use opl2_emulator::{Chip, init_tables};
use sdl2::AudioSubsystem;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use std::sync::{Arc, Mutex};
use wad::WadData;

const GENMIDI_HEADER: &str = "#OPL_II#";
const GENMIDI_NUM_INSTRS: usize = 128;
const GENMIDI_NUM_PERCUSSION: usize = 47;
const GENMIDI_FLAG_FIXED: u16 = 0x0001;
const GENMIDI_FLAG_2VOICE: u16 = 0x0004;

const OPL_CHANNELS: usize = 9;

const VOLUME_MAPPING_TABLE: [u8; 128] = [
    0, 1, 3, 5, 6, 8, 10, 11, 13, 14, 16, 17, 19, 20, 22, 23, 25, 26, 27, 29, 30, 32, 33, 34, 36,
    37, 39, 41, 43, 45, 47, 49, 50, 52, 54, 55, 57, 59, 60, 61, 63, 64, 66, 67, 68, 69, 71, 72, 73,
    74, 75, 76, 77, 79, 80, 81, 82, 83, 84, 84, 85, 86, 87, 88, 89, 90, 91, 92, 92, 93, 94, 95, 96,
    96, 97, 98, 99, 99, 100, 101, 101, 102, 103, 103, 104, 105, 105, 106, 107, 107, 108, 109, 109,
    110, 110, 111, 112, 112, 113, 113, 114, 114, 115, 115, 116, 117, 117, 118, 118, 119, 119, 120,
    120, 121, 121, 122, 122, 123, 123, 123, 124, 124, 125, 125, 126, 126, 127, 127,
];

const FREQUENCY_CURVE: [u16; 668] = [
    0x133, 0x133, 0x134, 0x134, 0x135, 0x136, 0x136, 0x137, 0x137, 0x138, 0x138, 0x139, 0x139,
    0x13a, 0x13b, 0x13b, 0x13c, 0x13c, 0x13d, 0x13d, 0x13e, 0x13f, 0x13f, 0x140, 0x140, 0x141,
    0x142, 0x142, 0x143, 0x143, 0x144, 0x144, 0x145, 0x146, 0x146, 0x147, 0x147, 0x148, 0x149,
    0x149, 0x14a, 0x14a, 0x14b, 0x14c, 0x14c, 0x14d, 0x14d, 0x14e, 0x14f, 0x14f, 0x150, 0x150,
    0x151, 0x152, 0x152, 0x153, 0x153, 0x154, 0x155, 0x155, 0x156, 0x157, 0x157, 0x158, 0x158,
    0x159, 0x15a, 0x15a, 0x15b, 0x15b, 0x15c, 0x15d, 0x15d, 0x15e, 0x15f, 0x15f, 0x160, 0x161,
    0x161, 0x162, 0x162, 0x163, 0x164, 0x164, 0x165, 0x166, 0x166, 0x167, 0x168, 0x168, 0x169,
    0x16a, 0x16a, 0x16b, 0x16c, 0x16c, 0x16d, 0x16e, 0x16e, 0x16f, 0x170, 0x170, 0x171, 0x172,
    0x172, 0x173, 0x174, 0x174, 0x175, 0x176, 0x176, 0x177, 0x178, 0x178, 0x179, 0x17a, 0x17a,
    0x17b, 0x17c, 0x17c, 0x17d, 0x17e, 0x17e, 0x17f, 0x180, 0x181, 0x181, 0x182, 0x183, 0x183,
    0x184, 0x185, 0x185, 0x186, 0x187, 0x188, 0x188, 0x189, 0x18a, 0x18a, 0x18b, 0x18c, 0x18d,
    0x18d, 0x18e, 0x18f, 0x18f, 0x190, 0x191, 0x192, 0x192, 0x193, 0x194, 0x194, 0x195, 0x196,
    0x197, 0x197, 0x198, 0x199, 0x19a, 0x19a, 0x19b, 0x19c, 0x19d, 0x19d, 0x19e, 0x19f, 0x1a0,
    0x1a0, 0x1a1, 0x1a2, 0x1a3, 0x1a3, 0x1a4, 0x1a5, 0x1a6, 0x1a6, 0x1a7, 0x1a8, 0x1a9, 0x1a9,
    0x1aa, 0x1ab, 0x1ac, 0x1ad, 0x1ad, 0x1ae, 0x1af, 0x1b0, 0x1b0, 0x1b1, 0x1b2, 0x1b3, 0x1b4,
    0x1b4, 0x1b5, 0x1b6, 0x1b7, 0x1b8, 0x1b8, 0x1b9, 0x1ba, 0x1bb, 0x1bc, 0x1bc, 0x1bd, 0x1be,
    0x1bf, 0x1c0, 0x1c0, 0x1c1, 0x1c2, 0x1c3, 0x1c4, 0x1c4, 0x1c5, 0x1c6, 0x1c7, 0x1c8, 0x1c9,
    0x1c9, 0x1ca, 0x1cb, 0x1cc, 0x1cd, 0x1ce, 0x1ce, 0x1cf, 0x1d0, 0x1d1, 0x1d2, 0x1d3, 0x1d3,
    0x1d4, 0x1d5, 0x1d6, 0x1d7, 0x1d8, 0x1d8, 0x1d9, 0x1da, 0x1db, 0x1dc, 0x1dd, 0x1de, 0x1de,
    0x1df, 0x1e0, 0x1e1, 0x1e2, 0x1e3, 0x1e4, 0x1e5, 0x1e5, 0x1e6, 0x1e7, 0x1e8, 0x1e9, 0x1ea,
    0x1eb, 0x1ec, 0x1ed, 0x1ed, 0x1ee, 0x1ef, 0x1f0, 0x1f1, 0x1f2, 0x1f3, 0x1f4, 0x1f5, 0x1f6,
    0x1f6, 0x1f7, 0x1f8, 0x1f9, 0x1fa, 0x1fb, 0x1fc, 0x1fd, 0x1fe, 0x1ff, 0x200, 0x201, 0x201,
    0x202, 0x203, 0x204, 0x205, 0x206, 0x207, 0x208, 0x209, 0x20a, 0x20b, 0x20c, 0x20d, 0x20e,
    0x20f, 0x210, 0x210, 0x211, 0x212, 0x213, 0x214, 0x215, 0x216, 0x217, 0x218, 0x219, 0x21a,
    0x21b, 0x21c, 0x21d, 0x21e, 0x21f, 0x220, 0x221, 0x222, 0x223, 0x224, 0x225, 0x226, 0x227,
    0x228, 0x229, 0x22a, 0x22b, 0x22c, 0x22d, 0x22e, 0x22f, 0x230, 0x231, 0x232, 0x233, 0x234,
    0x235, 0x236, 0x237, 0x238, 0x239, 0x23a, 0x23b, 0x23c, 0x23d, 0x23e, 0x23f, 0x240, 0x241,
    0x242, 0x244, 0x245, 0x246, 0x247, 0x248, 0x249, 0x24a, 0x24b, 0x24c, 0x24d, 0x24e, 0x24f,
    0x250, 0x251, 0x252, 0x253, 0x254, 0x256, 0x257, 0x258, 0x259, 0x25a, 0x25b, 0x25c, 0x25d,
    0x25e, 0x25f, 0x260, 0x262, 0x263, 0x264, 0x265, 0x266, 0x267, 0x268, 0x269, 0x26a, 0x26c,
    0x26d, 0x26e, 0x26f, 0x270, 0x271, 0x272, 0x273, 0x275, 0x276, 0x277, 0x278, 0x279, 0x27a,
    0x27b, 0x27d, 0x27e, 0x27f, 0x280, 0x281, 0x282, 0x284, 0x285, 0x286, 0x287, 0x288, 0x289,
    0x28b, 0x28c, 0x28d, 0x28e, 0x28f, 0x290, 0x292, 0x293, 0x294, 0x295, 0x296, 0x298, 0x299,
    0x29a, 0x29b, 0x29c, 0x29e, 0x29f, 0x2a0, 0x2a1, 0x2a2, 0x2a4, 0x2a5, 0x2a6, 0x2a7, 0x2a9,
    0x2aa, 0x2ab, 0x2ac, 0x2ae, 0x2af, 0x2b0, 0x2b1, 0x2b2, 0x2b4, 0x2b5, 0x2b6, 0x2b7, 0x2b9,
    0x2ba, 0x2bb, 0x2bd, 0x2be, 0x2bf, 0x2c0, 0x2c2, 0x2c3, 0x2c4, 0x2c5, 0x2c7, 0x2c8, 0x2c9,
    0x2cb, 0x2cc, 0x2cd, 0x2ce, 0x2d0, 0x2d1, 0x2d2, 0x2d4, 0x2d5, 0x2d6, 0x2d8, 0x2d9, 0x2da,
    0x2dc, 0x2dd, 0x2de, 0x2e0, 0x2e1, 0x2e2, 0x2e4, 0x2e5, 0x2e6, 0x2e8, 0x2e9, 0x2ea, 0x2ec,
    0x2ed, 0x2ee, 0x2f0, 0x2f1, 0x2f2, 0x2f4, 0x2f5, 0x2f6, 0x2f8, 0x2f9, 0x2fb, 0x2fc, 0x2fd,
    0x2ff, 0x300, 0x302, 0x303, 0x304, 0x306, 0x307, 0x309, 0x30a, 0x30b, 0x30d, 0x30e, 0x310,
    0x311, 0x312, 0x314, 0x315, 0x317, 0x318, 0x31a, 0x31b, 0x31c, 0x31e, 0x31f, 0x321, 0x322,
    0x324, 0x325, 0x327, 0x328, 0x329, 0x32b, 0x32c, 0x32e, 0x32f, 0x331, 0x332, 0x334, 0x335,
    0x337, 0x338, 0x33a, 0x33b, 0x33d, 0x33e, 0x340, 0x341, 0x343, 0x344, 0x346, 0x347, 0x349,
    0x34a, 0x34c, 0x34d, 0x34f, 0x350, 0x352, 0x353, 0x355, 0x357, 0x358, 0x35a, 0x35b, 0x35d,
    0x35e, 0x360, 0x361, 0x363, 0x365, 0x366, 0x368, 0x369, 0x36b, 0x36c, 0x36e, 0x370, 0x371,
    0x373, 0x374, 0x376, 0x378, 0x379, 0x37b, 0x37c, 0x37e, 0x380, 0x381, 0x383, 0x384, 0x386,
    0x388, 0x389, 0x38b, 0x38d, 0x38e, 0x390, 0x392, 0x393, 0x395, 0x397, 0x398, 0x39a, 0x39c,
    0x39d, 0x39f, 0x3a1, 0x3a2, 0x3a4, 0x3a6, 0x3a7, 0x3a9, 0x3ab, 0x3ac, 0x3ae, 0x3b0, 0x3b1,
    0x3b3, 0x3b5, 0x3b7, 0x3b8, 0x3ba, 0x3bc, 0x3bd, 0x3bf, 0x3c1, 0x3c3, 0x3c4, 0x3c6, 0x3c8,
    0x3ca, 0x3cb, 0x3cd, 0x3cf, 0x3d1, 0x3d2, 0x3d4, 0x3d6, 0x3d8, 0x3da, 0x3db, 0x3dd, 0x3df,
    0x3e1, 0x3e3, 0x3e4, 0x3e6, 0x3e8, 0x3ea, 0x3ec, 0x3ed, 0x3ef, 0x3f1, 0x3f3, 0x3f5, 0x3f6,
    0x3f8, 0x3fa, 0x3fc, 0x3fe, 0x36c,
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

#[derive(Debug, Clone, Copy)]
struct ChannelData {
    volume: u8,
    program: u8,
    sustain: bool,
    bend: i32,
}

impl Default for ChannelData {
    fn default() -> Self {
        Self {
            volume: 127,
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

struct OplPlayerState {
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
    music_volume: u8,
    volume: i32,
}

impl OplPlayerState {
    fn new(sample_rate: u32, use_opl3: bool, wad: &WadData) -> Self {
        init_tables();
        let mut chip = Chip::new(use_opl3);
        chip.setup(sample_rate);

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
            music_volume: 100,
            volume: 64,
        }
    }

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

    fn load_music(&mut self, midi_data: Vec<u8>) -> Result<(), String> {
        self.midi_track = MidiTrack::new();
        self.parse_midi(&midi_data)?;
        self.calculate_timing();
        Ok(())
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

    fn start_playback(&mut self, looping: bool) {
        self.is_playing = true;
        self.loop_music = looping;
        self.midi_track.reset();
        self.sample_counter = 0.0;
        self.stop_all_notes();
        self.channels = [ChannelData::default(); 16];
    }

    fn stop_playback(&mut self) {
        self.is_playing = false;
        self.stop_all_notes();
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

    fn set_voice_volume(
        &mut self,
        opl_channel: usize,
        note_volume: u8,
        channel_volume: u8,
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

        let mapped_note_vol = VOLUME_MAPPING_TABLE[note_volume.min(127) as usize];
        let mapped_chan_vol = VOLUME_MAPPING_TABLE[channel_volume.min(127) as usize];
        let mapped_music_vol = VOLUME_MAPPING_TABLE[self.music_volume.min(127) as usize];

        let full_volume =
            (mapped_note_vol as u32 * mapped_chan_vol as u32 * mapped_music_vol as u32)
                / (127 * 127);

        let op_volume = 0x3F - (voice.carrier.level & 0x3F);
        let reg_volume = ((op_volume as u32 * full_volume) / 128) as u8;
        let final_volume = (0x3F - reg_volume) | (voice.carrier.scale & 0xC0);

        self.chip.write_reg(0x40 + op2_offset as u32, final_volume);

        if (voice.feedback & 0x01) != 0 {
            self.chip.write_reg(0x40 + op1_offset as u32, final_volume);
        }

        final_volume
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
            let offset = voice.base_note_offset;
            let note_with_offset = (note as i32 + offset as i32) as u8;

            if note_with_offset > 0x7F {
                note
            } else {
                note_with_offset
            }
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
                    // Note off
                    self.note_off(event.channel, event.data1);
                }
                0x90 => {
                    // Note on
                    if event.data2 > 0 {
                        let program = self.channels[event.channel as usize].program;
                        self.note_on(event.channel, event.data1, event.data2, program);
                    } else {
                        self.note_off(event.channel, event.data1);
                    }
                }
                0xB0 => {
                    // Controller
                    let event_channel = event.channel;
                    match event.data1 {
                        7 => {
                            // Channel volume
                            let new_volume = event.data2;
                            self.channels[event_channel as usize].volume = new_volume;

                            // Update all active voices on this channel
                            let updates: Vec<_> = self
                                .playing_notes
                                .iter()
                                .filter(|n| n.midi_channel == event_channel)
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
                                    new_volume,
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
                        64 => {
                            // Sustain pedal (CC64)
                            let sustain_on = event.data2 >= 64;
                            self.channels[event_channel as usize].sustain = sustain_on;

                            if !sustain_on {
                                self.release_sustained_notes(event_channel);
                            }
                        }
                        120 => {
                            // All Sound Off (CC120)
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
                            // All Notes Off (CC123)
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
                    // Program change
                    self.channels[event.channel as usize].program = event.data1;
                }
                0xE0 => {
                    // Pitch bend - only use MSB like Doom does
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
            } else {
                self.is_playing = false;
            }
        }
    }

    fn generate_samples(&mut self, buffer: &mut [i16]) {
        if !self.is_playing {
            buffer.fill(0);
            return;
        }

        for sample in buffer.iter_mut() {
            // Update MIDI timing
            self.sample_counter += 1.0;
            while self.sample_counter >= self.samples_per_tick {
                self.sample_counter -= self.samples_per_tick;
                self.midi_track.tick_time += 1;
                self.process_midi_events();
            }

            // Generate OPL sample
            let mut opl_sample = [0i32; 1];
            self.chip.generate_block_2(1, &mut opl_sample);

            // Apply volume and clamp to i16 range
            let scaled = (opl_sample[0] * self.volume) / 128;
            *sample = scaled.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        }
    }
}

struct OplAudioCallback {
    state: Arc<Mutex<OplPlayerState>>,
    volume: Arc<Mutex<i32>>,
}

impl AudioCallback for OplAudioCallback {
    type Channel = i16;

    fn callback(&mut self, out: &mut [i16]) {
        if let Ok(mut state) = self.state.lock() {
            if let Ok(volume) = self.volume.lock() {
                state.volume = *volume;
            }
            state.generate_samples(out);
        } else {
            out.fill(0);
        }
    }
}

pub struct OplPlayer {
    _device: AudioDevice<OplAudioCallback>,
    state: Arc<Mutex<OplPlayerState>>,
    volume: Arc<Mutex<i32>>,
}

impl OplPlayer {
    pub fn new(audio: &AudioSubsystem, use_opl3: bool, wad: &WadData) -> Result<Self, String> {
        let desired_spec = AudioSpecDesired {
            freq: Some(44100),
            channels: Some(1),
            samples: Some(512),
        };

        let state = Arc::new(Mutex::new(OplPlayerState::new(44100, use_opl3, wad)));
        let callback_state = Arc::clone(&state);
        let volume = Arc::new(Mutex::new(64));
        let callback_volume = Arc::clone(&volume);

        let device = audio.open_playback(None, &desired_spec, |_spec| OplAudioCallback {
            state: callback_state,
            volume: callback_volume,
        })?;

        Ok(Self {
            _device: device,
            state,
            volume,
        })
    }

    pub fn load_music(&mut self, midi_data: Vec<u8>) -> Result<(), String> {
        if let Ok(mut state) = self.state.lock() {
            state.load_music(midi_data)
        } else {
            Err("Failed to lock OPL player state".to_string())
        }
    }

    pub fn play(&mut self, looping: bool) -> Result<(), String> {
        if let Ok(mut state) = self.state.lock() {
            state.start_playback(looping);
            self._device.resume();
            Ok(())
        } else {
            Err("Failed to lock OPL player state".to_string())
        }
    }

    pub fn stop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            state.stop_playback();
        }
        self._device.pause();
    }

    pub fn pause(&mut self) {
        self._device.pause();
    }

    pub fn resume(&mut self) {
        self._device.resume();
    }

    pub fn set_volume(&mut self, volume: i32) {
        if let Ok(mut vol) = self.volume.lock() {
            *vol = volume.clamp(0, 128);
        }

        if let Ok(mut state) = self.state.lock() {
            let music_vol = ((volume * 127) / 128).clamp(0, 127) as u8;
            state.music_volume = music_vol;

            // Update all active voices
            let updates: Vec<_> = state
                .playing_notes
                .iter()
                .map(|n| {
                    (
                        n.opl_channel,
                        n.note_volume,
                        state.channels[n.midi_channel as usize].volume,
                        n.current_instrument,
                        n.instrument_voice,
                    )
                })
                .collect();

            for (opl_channel, note_volume, channel_volume, instrument, voice_num) in updates {
                let reg_volume = state.set_voice_volume(
                    opl_channel,
                    note_volume,
                    channel_volume,
                    &instrument,
                    voice_num,
                );

                for note in &mut state.playing_notes {
                    if note.opl_channel == opl_channel {
                        note.reg_volume = reg_volume;
                        break;
                    }
                }
            }
        }
    }

    pub fn get_volume(&self) -> i32 {
        self.volume.lock().map(|v| *v).unwrap_or(64)
    }

    pub fn is_playing(&self) -> Result<bool, String> {
        if let Ok(state) = self.state.lock() {
            Ok(state.is_playing)
        } else {
            Err("Failed to lock OPL player state".to_string())
        }
    }
}
