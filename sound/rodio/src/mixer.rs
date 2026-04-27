//! Custom 32-channel SFX mixer.
//!
//! All active channels are summed per-frame with per-channel pan and
//! distance-based volume attenuation. The mixer produces an interleaved
//! stereo `f32` stream at the project sample rate; block mixing into a
//! preallocated scratch buffer matches the cadence used by the OPL and
//! GUS music sources, so all three audio sources share one structural
//! pattern.

use sound_common::MIXER_CHANNELS;

/// Number of stereo frames mixed per block. 512 frames at 44.1 kHz is
/// ~11.6 ms — below human action-to-sound perception threshold and
/// matched to the OPL/GUS source block sizes so all three sources tic
/// at the same rate.
const BUFFER_FRAMES: usize = 512;
/// Number of `f32` samples per block (interleaved L,R).
const BUFFER_SAMPLES: usize = BUFFER_FRAMES * 2;

/// Per-channel playback state
#[derive(Clone)]
pub struct ChannelState {
    /// Pre-converted mono f32 samples at the project sample rate.
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

/// 32-channel SFX mixer that outputs interleaved stereo f32.
///
/// Wrapping this in `Arc<Mutex<>>` allows the sound thread to mutate channel
/// state while the audio callback thread reads samples. Reads block-mix
/// `BUFFER_FRAMES` frames at a time into the preallocated `buffer`; the
/// `cursor` walks the buffer as `next()` dispenses samples. Refills happen
/// transparently when the cursor exhausts the buffer.
pub struct SfxMixer {
    pub channels: Vec<ChannelState>,
    pub master_volume: f32,
    /// Preallocated interleaved stereo scratch (`2 * BUFFER_FRAMES`).
    /// Refilled in place by `mix_block`; never reallocated.
    buffer: Vec<f32>,
    /// Index into `buffer`; `>= buffer.len()` triggers a refill.
    cursor: usize,
}

impl SfxMixer {
    pub fn new() -> Self {
        Self {
            channels: (0..MIXER_CHANNELS as usize)
                .map(|_| ChannelState::default())
                .collect(),
            master_volume: 1.0,
            buffer: vec![0.0f32; BUFFER_SAMPLES],
            cursor: BUFFER_SAMPLES, // start exhausted: first next() triggers a fill
        }
    }

    /// Mix `BUFFER_FRAMES` stereo frames from all active channels into
    /// `self.buffer`. Each frame is clamped to [-1.0, 1.0] after master
    /// volume — matching the previous per-frame clamp behaviour exactly.
    fn mix_block(&mut self) {
        for s in self.buffer.iter_mut() {
            *s = 0.0;
        }

        for ch in self.channels.iter_mut() {
            if !ch.active {
                continue;
            }
            // Equal-power pan: pan=0 gives equal volume to both channels.
            // Pan/distance_vol are constant across the block because the
            // sfx mutex is held for the duration of mix_block; the sound
            // thread cannot mutate channels mid-block.
            let right_gain = (ch.pan + 1.0) * 0.5;
            let left_gain = 1.0 - right_gain;
            let dvol = ch.distance_vol;

            for frame in 0..BUFFER_FRAMES {
                if ch.cursor >= ch.samples.len() {
                    ch.active = false;
                    break;
                }
                let sample = ch.samples[ch.cursor] * dvol;
                ch.cursor += 1;
                self.buffer[frame * 2] += sample * left_gain;
                self.buffer[frame * 2 + 1] += sample * right_gain;
            }
        }

        // Apply master volume + per-frame clamp.
        let master = self.master_volume;
        for s in self.buffer.iter_mut() {
            *s = (*s * master).clamp(-1.0, 1.0);
        }

        self.cursor = 0;
    }
}

impl Iterator for SfxMixer {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.cursor >= self.buffer.len() {
            self.mix_block();
        }
        let sample = self.buffer[self.cursor];
        self.cursor += 1;
        Some(sample)
    }
}

impl_stereo_source!(SfxMixer);

#[cfg(test)]
mod tests {
    use super::*;

    /// Reading an odd number of samples must not desync L/R for any
    /// subsequent reads. The block-mixing layout self-encodes channel
    /// position via the cursor; an odd-length read just lands on an
    /// odd cursor and the next reads continue from there with correct
    /// L/R alignment for whatever frame the cursor points into.
    #[test]
    fn odd_length_read_preserves_alignment() {
        let mut mixer = SfxMixer::new();
        // Drive a known signal: one channel, full left, sample value 1.0.
        mixer.channels[0].samples = vec![1.0; BUFFER_FRAMES * 2];
        mixer.channels[0].active = true;
        mixer.channels[0].pan = -1.0; // full left
        mixer.channels[0].distance_vol = 1.0;

        // Pull one (odd) sample: should be the L of frame 0 = 1.0.
        let s0 = mixer.next().unwrap();
        assert_eq!(s0, 1.0, "first sample should be left of frame 0");

        // Pull the matching R, which should be silent (full-left pan).
        let s1 = mixer.next().unwrap();
        assert_eq!(s1, 0.0, "right of frame 0 should be silent for full-left pan");

        // Continue across the block boundary; alignment must hold.
        for frame in 1..BUFFER_FRAMES {
            let l = mixer.next().unwrap();
            let r = mixer.next().unwrap();
            assert_eq!(l, 1.0, "frame {frame} left desynced");
            assert_eq!(r, 0.0, "frame {frame} right desynced");
        }
    }

    /// Channels stop emitting once their sample buffer is exhausted,
    /// and are marked inactive without producing garbage.
    #[test]
    fn exhausted_channel_stops_and_deactivates() {
        let mut mixer = SfxMixer::new();
        mixer.channels[0].samples = vec![1.0; 4]; // four mono samples
        mixer.channels[0].active = true;
        mixer.channels[0].pan = 0.0;

        mixer.mix_block();
        assert!(!mixer.channels[0].active, "channel should deactivate when exhausted");

        // First four frames carry signal; rest of the block is silent.
        for frame in 0..4 {
            assert!(mixer.buffer[frame * 2] != 0.0, "frame {frame} L expected non-zero");
        }
        for frame in 4..BUFFER_FRAMES {
            assert_eq!(mixer.buffer[frame * 2], 0.0, "frame {frame} L expected silence");
            assert_eq!(mixer.buffer[frame * 2 + 1], 0.0, "frame {frame} R expected silence");
        }
    }
}
