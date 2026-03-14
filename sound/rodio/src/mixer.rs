//! Custom 32-channel audio mixer implementing rodio's `Source` trait.
//!
//! All active channels are summed each sample with per-channel pan and
//! distance-based volume attenuation.

use std::time::Duration;

use rodio::Source;
use sound_common::MIXER_CHANNELS;

const SAMPLE_RATE: u32 = 44_100;

/// Per-channel playback state
#[derive(Clone)]
pub struct ChannelState {
    /// Pre-converted mono f32 samples at 44100 Hz
    pub samples: Vec<f32>,
    /// Current playback position (in mono samples)
    pub cursor: usize,
    /// Whether this channel is actively playing
    pub active: bool,
    /// Playback priority (higher = harder to evict)
    pub priority: i32,
    /// Stereo pan: -1.0 = full left, 0.0 = center, 1.0 = full right
    pub pan: f32,
    /// Distance-based volume: 0.0 = silent, 1.0 = max
    pub distance_vol: f32,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            cursor: 0,
            active: false,
            priority: 0,
            pan: 0.0,
            distance_vol: 1.0,
        }
    }
}

/// Doom-style 32-channel mixer that outputs interleaved stereo f32.
///
/// Wrapping this in `Arc<Mutex<>>` allows the sound thread to mutate channel
/// state while the audio callback thread reads samples.
pub struct DoomMixer {
    pub channels: Vec<ChannelState>,
    pub master_volume: f32,
    /// Tracks stereo output position: false = left sample next, true = right
    right_phase: bool,
    /// Cached left sample (computed when left phase runs, emitted on right)
    cached_left: f32,
    cached_right: f32,
}

impl DoomMixer {
    pub fn new() -> Self {
        Self {
            channels: (0..MIXER_CHANNELS as usize)
                .map(|_| ChannelState::default())
                .collect(),
            master_volume: 1.0,
            right_phase: false,
            cached_left: 0.0,
            cached_right: 0.0,
        }
    }

    /// Mix one stereo frame from all active channels
    fn mix_frame(&mut self) {
        let mut left = 0.0f32;
        let mut right = 0.0f32;

        for ch in self.channels.iter_mut() {
            if !ch.active {
                continue;
            }
            if ch.cursor >= ch.samples.len() {
                ch.active = false;
                continue;
            }

            let sample = ch.samples[ch.cursor] * ch.distance_vol;
            ch.cursor += 1;

            // Equal-power pan: pan=0 gives equal volume to both channels
            let right_gain = (ch.pan + 1.0) * 0.5;
            let left_gain = 1.0 - right_gain;

            left += sample * left_gain;
            right += sample * right_gain;
        }

        self.cached_left = (left * self.master_volume).clamp(-1.0, 1.0);
        self.cached_right = (right * self.master_volume).clamp(-1.0, 1.0);
    }
}

impl Iterator for DoomMixer {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if !self.right_phase {
            self.mix_frame();
            self.right_phase = true;
            Some(self.cached_left)
        } else {
            self.right_phase = false;
            Some(self.cached_right)
        }
    }
}

impl Source for DoomMixer {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        2
    }

    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
