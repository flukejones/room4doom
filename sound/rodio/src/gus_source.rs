//! GUS music playback via rustysynth SF2 synthesizer.
//!
//! Wraps a `MidiFileSequencer` as a `rodio::Source`. The sequencer renders
//! stereo f32 samples from a loaded MIDI file using a SoundFont.

use std::fs::File;
use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc as StdArc, Arc, Mutex};

use log::warn;
use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};
use sound_common::SAMPLE_RATE;

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
///
/// `left`, `right`, and `buffer` are preallocated once; `refill` reuses
/// them in place to avoid per-frame heap traffic on the audio thread
/// (called every ~12 ms at 44.1 kHz).
pub struct GusSource {
    state: Arc<Mutex<GusPlayerState>>,
    /// Reusable left scratch buffer (`BUFFER_SIZE` samples).
    left: Vec<f32>,
    /// Reusable right scratch buffer (`BUFFER_SIZE` samples).
    right: Vec<f32>,
    /// Reusable interleaved stereo output (`2 * BUFFER_SIZE` samples).
    buffer: Vec<f32>,
    cursor: usize,
}

impl GusSource {
    pub fn new(state: Arc<Mutex<GusPlayerState>>) -> Self {
        Self {
            state,
            left: vec![0.0f32; BUFFER_SIZE],
            right: vec![0.0f32; BUFFER_SIZE],
            buffer: vec![0.0f32; BUFFER_SIZE * 2],
            cursor: 0,
        }
    }

    fn refill(&mut self) {
        for s in self.left.iter_mut() {
            *s = 0.0;
        }
        for s in self.right.iter_mut() {
            *s = 0.0;
        }
        match self.state.lock() {
            Ok(mut state) => state.generate_samples(&mut self.left, &mut self.right),
            Err(e) => warn!("GUS state mutex poisoned, emitting silence: {e}"),
        }

        // Interleave stereo (in-place into preallocated buffer)
        for i in 0..BUFFER_SIZE {
            self.buffer[i * 2] = self.left[i];
            self.buffer[i * 2 + 1] = self.right[i];
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

impl_stereo_source!(GusSource);
