//! Wraps `OplPlayerState` as a `rodio::Source` for music playback.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use opl2_emulator::OplPlayerState;
use rodio::Source;

const SAMPLE_RATE: u32 = 44_100;
const BUFFER_SIZE: usize = 512;

/// Rodio `Source` that generates OPL music samples.
///
/// Mono i16 from `OplPlayerState` is converted to stereo f32 output.
pub struct OplSource {
    state: Arc<Mutex<OplPlayerState>>,
    buffer: Vec<f32>,
    cursor: usize,
}

impl OplSource {
    /// Create a new OPL source wrapping a shared player state.
    pub fn new(state: Arc<Mutex<OplPlayerState>>) -> Self {
        Self {
            state,
            buffer: Vec::new(),
            cursor: 0,
        }
    }

    fn refill(&mut self) {
        let mut mono_i16 = vec![0i16; BUFFER_SIZE];
        if let Ok(mut state) = self.state.lock() {
            state.generate_samples(&mut mono_i16);
        }

        // Convert mono i16 → stereo f32
        self.buffer.clear();
        self.buffer.reserve(BUFFER_SIZE * 2);
        for &s in &mono_i16 {
            let f = s as f32 / 32768.0;
            self.buffer.push(f);
            self.buffer.push(f);
        }
        self.cursor = 0;
    }
}

impl Iterator for OplSource {
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

impl Source for OplSource {
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
