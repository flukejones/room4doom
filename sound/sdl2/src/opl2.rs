use opl2_emulator::{Chip, init_tables};
use sdl2::AudioSubsystem;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use std::sync::{Arc, Mutex};

const OPL_CHANNELS: usize = 9;

// Default GENMIDI-compatible instrument (basic sine wave)
const DEFAULT_INSTRUMENT: [u8; 36] = [
    0x00, 0x00, 0x1f, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x00, 0x00, 0x1f, 0x07,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x00, 0x00, 0xc0, 0xc0, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,
];

#[derive(Debug, Clone, Copy)]
struct MidiEvent {
    delta_time: u32,
    event_type: u8,
    channel: u8,
    data1: u8,
    data2: u8,
}

#[derive(Debug, Clone)]
struct OplNote {
    midi_channel: u8,
    midi_note: u8,
    opl_channel: usize,
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
    instruments: [[u8; 36]; 128],
}

impl OplPlayerState {
    fn new(sample_rate: u32, use_opl3: bool) -> Self {
        init_tables();
        let mut chip = Chip::new(use_opl3);
        chip.setup(sample_rate);

        // Initialize with default instruments
        let mut instruments = [[0u8; 36]; 128];
        for i in 0..128 {
            instruments[i] = DEFAULT_INSTRUMENT;
        }

        Self {
            chip,
            midi_track: MidiTrack::new(),
            playing_notes: Vec::new(),
            opl_channels: [false; OPL_CHANNELS],
            sample_rate,
            is_playing: false,
            loop_music: false,
            tempo: 500000, // microseconds per quarter note (120 BPM)
            ticks_per_quarter: 96,
            samples_per_tick: 0.0,
            sample_counter: 0.0,
            instruments,
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

        // Check for MIDI header
        if &data[0..4] != b"MThd" {
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
        let _num_tracks = u16::from_be_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        self.ticks_per_quarter = u16::from_be_bytes([data[pos], data[pos + 1]]) as u32;
        pos += 2;

        // Skip any extra header data
        pos += (header_length - 6) as usize;

        // Parse first track
        if pos + 8 > data.len() || &data[pos..pos + 4] != b"MTrk" {
            return Err("No valid track found".to_string());
        }

        pos += 4;
        let track_length =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let track_end = pos + track_length;
        let mut running_status = 0u8;

        while pos < track_end {
            // Parse delta time
            let (delta_time, new_pos) = self.parse_variable_length(&data, pos)?;
            pos = new_pos;

            if pos >= track_end {
                break;
            }

            let mut status = data[pos];
            if status < 0x80 {
                // Running status
                status = running_status;
            } else {
                pos += 1;
                running_status = status;
            }

            let event_type = status & 0xF0;
            let channel = status & 0x0F;

            match event_type {
                0x80 | 0x90 | 0xB0 | 0xC0 | 0xE0 => {
                    // Note events and controllers
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
                        delta_time,
                        event_type,
                        channel,
                        data1,
                        data2,
                    });
                }
                0xFF => {
                    // Meta event
                    if pos >= track_end {
                        break;
                    }
                    let meta_type = data[pos];
                    pos += 1;

                    let (length, new_pos) = self.parse_variable_length(&data, pos)?;
                    pos = new_pos;

                    if meta_type == 0x51 && length >= 3 {
                        // Set tempo
                        self.tempo =
                            u32::from_be_bytes([0, data[pos], data[pos + 1], data[pos + 2]]);
                    }

                    pos += length as usize;
                }
                _ => {
                    // Skip unknown events
                    pos += 1;
                }
            }
        }

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

    fn free_opl_channel(&mut self, channel: usize) {
        if channel < OPL_CHANNELS {
            self.opl_channels[channel] = false;
            self.chip.write_reg(0xB0 + channel as u32, 0);
        }
    }

    fn setup_instrument(&mut self, opl_channel: usize, program: u8) {
        let instrument = &self.instruments[program as usize];

        let op1_offset = if opl_channel < 3 {
            opl_channel
        } else {
            opl_channel + 5
        };
        let op2_offset = op1_offset + 3;

        // Setup operator 1
        self.chip.write_reg(0x20 + op1_offset as u32, instrument[0]); // AM/VIB/EG/KSR/MULT
        self.chip.write_reg(0x40 + op1_offset as u32, instrument[2]); // KSL/TL
        self.chip.write_reg(0x60 + op1_offset as u32, instrument[4]); // AR/DR
        self.chip.write_reg(0x80 + op1_offset as u32, instrument[6]); // SL/RR
        self.chip.write_reg(0xE0 + op1_offset as u32, instrument[8]); // WS

        // Setup operator 2
        self.chip.write_reg(0x20 + op2_offset as u32, instrument[1]); // AM/VIB/EG/KSR/MULT
        self.chip.write_reg(0x40 + op2_offset as u32, instrument[3]); // KSL/TL
        self.chip.write_reg(0x60 + op2_offset as u32, instrument[5]); // AR/DR
        self.chip.write_reg(0x80 + op2_offset as u32, instrument[7]); // SL/RR
        self.chip.write_reg(0xE0 + op2_offset as u32, instrument[9]); // WS

        // Setup channel
        self.chip
            .write_reg(0xC0 + opl_channel as u32, instrument[10]); // FB/CON
    }

    fn note_on(&mut self, channel: u8, note: u8, _velocity: u8, program: u8) {
        if let Some(opl_channel) = self.allocate_opl_channel() {
            self.setup_instrument(opl_channel, program);

            // Calculate frequency
            let freq = Self::midi_note_to_opl_freq(note);
            let freq_low = (freq & 0xFF) as u8;
            let freq_high = ((freq >> 8) & 0x1F) as u8;

            // Set frequency
            self.chip.write_reg(0xA0 + opl_channel as u32, freq_low);

            // Key on with octave
            let octave = Self::midi_note_to_octave(note);
            self.chip
                .write_reg(0xB0 + opl_channel as u32, freq_high | (octave << 2) | 0x20);

            self.playing_notes.push(OplNote {
                midi_channel: channel,
                midi_note: note,
                opl_channel,
            });
        }
    }

    fn note_off(&mut self, channel: u8, note: u8) {
        if let Some(pos) = self
            .playing_notes
            .iter()
            .position(|n| n.midi_channel == channel && n.midi_note == note)
        {
            let opl_note = self.playing_notes.remove(pos);
            self.free_opl_channel(opl_note.opl_channel);
        }
    }

    fn midi_note_to_opl_freq(note: u8) -> u16 {
        // Convert MIDI note to OPL frequency value
        let freq_table = [345, 365, 387, 410, 435, 460, 488, 517, 547, 580, 615, 651];
        let octave = note / 12;
        let note_in_octave = note % 12;
        let base_freq = freq_table[note_in_octave as usize];

        if octave > 0 {
            base_freq >> (octave - 1)
        } else {
            base_freq << 1
        }
    }

    fn midi_note_to_octave(note: u8) -> u8 {
        (note / 12).clamp(0, 7)
    }

    fn process_midi_events(&mut self) {
        while self.midi_track.position < self.midi_track.events.len() {
            let event = &self.midi_track.events[self.midi_track.position];

            if self.midi_track.tick_time < event.delta_time {
                break;
            }

            match event.event_type {
                0x80 => {
                    // Note off
                    self.note_off(event.channel, event.data1);
                }
                0x90 => {
                    // Note on
                    if event.data2 > 0 {
                        self.note_on(event.channel, event.data1, event.data2, 0);
                    } else {
                        self.note_off(event.channel, event.data1);
                    }
                }
                0xC0 => {
                    // Program change - could store per channel
                }
                _ => {}
            }

            self.midi_track.position += 1;
        }

        // Check if track finished
        if self.midi_track.position >= self.midi_track.events.len() {
            if self.loop_music {
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
            if self.sample_counter >= self.samples_per_tick {
                self.sample_counter -= self.samples_per_tick;
                self.midi_track.tick_time += 1;
                self.process_midi_events();
            }

            // Generate OPL sample
            let mut opl_sample = [0i32; 1];
            self.chip.generate_block_2(1, &mut opl_sample);
            *sample = (opl_sample[0] >> 8) as i16;
        }
    }
}

struct OplAudioCallback {
    state: Arc<Mutex<OplPlayerState>>,
}

impl AudioCallback for OplAudioCallback {
    type Channel = i16;

    fn callback(&mut self, out: &mut [i16]) {
        if let Ok(mut state) = self.state.lock() {
            state.generate_samples(out);
        } else {
            out.fill(0);
        }
    }
}

pub struct OplPlayer {
    _device: AudioDevice<OplAudioCallback>,
    state: Arc<Mutex<OplPlayerState>>,
    volume: i32,
}

impl OplPlayer {
    pub fn new(audio: &AudioSubsystem, use_opl3: bool) -> Result<Self, String> {
        let desired_spec = AudioSpecDesired {
            freq: Some(44100),
            channels: Some(1),
            samples: Some(512),
        };

        let state = Arc::new(Mutex::new(OplPlayerState::new(44100, use_opl3)));
        let callback_state = Arc::clone(&state);

        let device = audio.open_playback(None, &desired_spec, |_spec| OplAudioCallback {
            state: callback_state,
        })?;

        Ok(Self {
            _device: device,
            state,
            volume: 64,
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
        self.volume = volume.clamp(0, 128);
    }

    pub fn get_volume(&self) -> i32 {
        self.volume
    }

    pub fn is_playing(&self) -> Result<bool, String> {
        if let Ok(state) = self.state.lock() {
            Ok(state.is_playing)
        } else {
            Err("Failed to lock OPL player state".to_string())
        }
    }
}
