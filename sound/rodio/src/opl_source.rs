//! Wraps `OplPlayerState` as a `rodio::Source` for music playback.

use std::sync::{Arc, Mutex};

use log::warn;
use opl2_emulator::OplPlayerState;

const BUFFER_SIZE: usize = 512;

/// Rodio `Source` that generates OPL music samples.
///
/// Mono i16 from `OplPlayerState` is converted to stereo f32 output.
/// `mono` and `buffer` are preallocated once; `refill` reuses the
/// buffers in place to avoid per-frame heap traffic on the audio
/// thread (called every ~12 ms at 44.1 kHz).
pub struct OplSource {
    state: Arc<Mutex<OplPlayerState>>,
    /// Reusable mono i16 scratch buffer (`BUFFER_SIZE` samples).
    mono: Vec<i16>,
    /// Reusable stereo f32 output buffer (`2 * BUFFER_SIZE` samples).
    buffer: Vec<f32>,
    cursor: usize,
}

impl OplSource {
    /// Create a new OPL source wrapping a shared player state.
    pub fn new(state: Arc<Mutex<OplPlayerState>>) -> Self {
        Self {
            state,
            mono: vec![0i16; BUFFER_SIZE],
            buffer: vec![0.0f32; BUFFER_SIZE * 2],
            cursor: 0,
        }
    }

    fn refill(&mut self) {
        for s in self.mono.iter_mut() {
            *s = 0;
        }
        match self.state.lock() {
            Ok(mut state) => state.generate_samples(&mut self.mono),
            Err(e) => warn!("OPL state mutex poisoned, emitting silence: {e}"),
        }

        // Convert mono i16 → stereo f32 (in-place into preallocated buffer)
        for (i, &s) in self.mono.iter().enumerate() {
            let f = s as f32 / 32768.0;
            self.buffer[i * 2] = f;
            self.buffer[i * 2 + 1] = f;
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

impl_stereo_source!(OplSource);
