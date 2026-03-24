//! GUS music playback via rustysynth SF2 synthesizer.
//!
//! Wraps a `MidiFileSequencer` as a `rodio::Source`. The sequencer renders
//! stereo f32 samples from a loaded MIDI file using a SoundFont.

use std::fs::File;
use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::Source;
use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};
use std::sync::Arc as StdArc;

const SAMPLE_RATE: u32 = 44_100;
const BUFFER_SIZE: usize = 512;

pub struct GusPlayerState {
    sf2: Arc<SoundFont>,
    sequencer: MidiFileSequencer,
    midi: Option<StdArc<MidiFile>>,
    pub volume: i32,
}

impl GusPlayerState {
    pub fn new(sf2_path: &Path) -> Result<Self, String> {
        let mut file = File::open(sf2_path)
            .map_err(|e| format!("Failed to open SF2 {}: {e}", sf2_path.display()))?;
        let sf2 =
            Arc::new(SoundFont::new(&mut file).map_err(|e| format!("Failed to parse SF2: {e}"))?);
        let settings = SynthesizerSettings::new(SAMPLE_RATE as i32);
        let synth = Synthesizer::new(&sf2, &settings)
            .map_err(|e| format!("Failed to create synthesizer: {e}"))?;
        let sequencer = MidiFileSequencer::new(synth);

        Ok(Self {
            sf2,
            sequencer,
            midi: None,
            volume: 100,
        })
    }

    pub fn load_music(&mut self, midi_data: &[u8], looping: bool) -> Result<(), String> {
        let midi = StdArc::new(
            MidiFile::new(&mut Cursor::new(midi_data))
                .map_err(|e| format!("Failed to parse MIDI: {e}"))?,
        );
        // Reset synthesizer for clean playback
        let settings = SynthesizerSettings::new(SAMPLE_RATE as i32);
        let synth = Synthesizer::new(&self.sf2, &settings)
            .map_err(|e| format!("Failed to reset synthesizer: {e}"))?;
        self.sequencer = MidiFileSequencer::new(synth);
        self.midi = Some(StdArc::clone(&midi));
        self.sequencer.play(&midi, looping);
        Ok(())
    }

    pub fn start_playback(&mut self, looping: bool) {
        if let Some(ref midi) = self.midi {
            self.sequencer.play(midi, looping);
        }
    }

    pub fn stop_playback(&mut self) {
        self.sequencer.stop();
    }

    pub fn generate_samples(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.sequencer.render(left, right);
        let vol = self.volume as f32 / 128.0;
        for s in left.iter_mut() {
            *s *= vol;
        }
        for s in right.iter_mut() {
            *s *= vol;
        }
    }
}

/// Rodio `Source` that generates GUS/SF2 music samples.
pub struct GusSource {
    state: Arc<Mutex<GusPlayerState>>,
    buffer: Vec<f32>,
    cursor: usize,
}

impl GusSource {
    pub fn new(state: Arc<Mutex<GusPlayerState>>) -> Self {
        Self {
            state,
            buffer: Vec::new(),
            cursor: 0,
        }
    }

    fn refill(&mut self) {
        let mut left = vec![0.0f32; BUFFER_SIZE];
        let mut right = vec![0.0f32; BUFFER_SIZE];
        if let Ok(mut state) = self.state.lock() {
            state.generate_samples(&mut left, &mut right);
        }

        // Interleave stereo
        self.buffer.clear();
        self.buffer.reserve(BUFFER_SIZE * 2);
        for i in 0..BUFFER_SIZE {
            self.buffer.push(left[i]);
            self.buffer.push(right[i]);
        }
        self.cursor = 0;
    }
}

impl Iterator for GusSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.cursor >= self.buffer.len() {
            self.refill();
        }
        let sample = self.buffer[self.cursor];
        self.cursor += 1;
        Some(sample)
    }
}

impl Source for GusSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        rodio::ChannelCount::new(2).unwrap()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        rodio::SampleRate::new(SAMPLE_RATE).unwrap()
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
