//! Pure-Rust sound backend using rodio (cpal) for audio output.
//!
//! Replaces SDL2_mixer for SFX playback. Music support (OPL/rustysynth)
//! is handled by separate source modules added to the same output stream.

use std::error::Error;
use std::fmt::{self, Display};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use log::{debug, info, warn};
use opl2_emulator::OplPlayerState;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source};
use sound_common::{
    InitResult, MAX_DIST, MID_ID, MIXER_CHANNELS, MUS_DATA, MUS_ID, SFX_INFO_BASE, SfxName, SndServerRx, SndServerTx, SoundObject, SoundServer, SoundServerTic, dist_from_points, listener_to_source_angle_deg, read_mus_to_midi
};
use wad::WadData;

mod mixer;
use mixer::{ChannelState, DoomMixer};

mod opl_source;
use opl_source::OplSource;

const SAMPLE_RATE: u32 = 44_100;

/// Pre-loaded sound effect data
struct SfxChunk {
    /// Mono f32 samples at 44100 Hz
    samples: Vec<f32>,
    /// Playback priority
    priority: i32,
}

/// Deferred stream initialization data returned by `Snd::new`, created on the
/// sound thread via `init_stream`.
struct StreamState {
    _sink: MixerDeviceSink,
}

/// Pure-Rust sound server using rodio
pub struct Snd {
    rx: SndServerRx,
    tx: SndServerTx,
    mixer: Arc<Mutex<DoomMixer>>,
    opl_state: Arc<Mutex<OplPlayerState>>,
    stream: Option<StreamState>,
    chunks: Vec<SfxChunk>,
    /// Pre-converted MIDI data per music track
    midi_data: Vec<Vec<u8>>,
    listener: SoundObject<SfxName>,
    sources: [SoundObject<SfxName>; MIXER_CHANNELS as usize],
    sfx_vol: i32,
    mus_vol: i32,
}

unsafe impl Send for Snd {}

#[derive(Debug)]
pub enum SndError {
    Init(String),
}

impl Display for SndError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SndError::Init(msg) => write!(f, "Sound init error: {msg}"),
        }
    }
}

impl Error for SndError {}

/// Convert a WAD SFX lump (8-bit unsigned PCM) to f32 mono at 44100 Hz
fn lump_sfx_to_f32(raw_lump: &[u8]) -> Option<Vec<f32>> {
    if raw_lump.len() < 8 {
        return None;
    }

    let rate = i16::from_le_bytes([raw_lump[2], raw_lump[3]]) as f32;
    if rate <= 0.0 {
        return None;
    }

    let len = i32::from_le_bytes([raw_lump[4], raw_lump[5], raw_lump[6], raw_lump[7]]) as usize;
    let data_end = len.min(raw_lump.len());
    let pcm = &raw_lump[8..data_end];

    if pcm.is_empty() {
        return None;
    }

    // Convert u8 to f32 [-1, 1]
    let mono: Vec<f32> = pcm.iter().map(|&s| (s as f32 - 128.0) / 128.0).collect();

    // Resample to 44100 Hz via linear interpolation
    let ratio = SAMPLE_RATE as f64 / rate as f64;
    let out_len = (mono.len() as f64 * ratio) as usize;
    let mut resampled = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos as usize;
        let frac = (src_pos - idx as f64) as f32;

        let s0 = mono[idx.min(mono.len() - 1)];
        let s1 = mono[(idx + 1).min(mono.len() - 1)];
        resampled.push(s0 + (s1 - s0) * frac);
    }

    Some(resampled)
}

impl Snd {
    pub fn new(wad: &WadData) -> Result<Self, Box<dyn Error>> {
        let chunks: Vec<SfxChunk> = SFX_INFO_BASE
            .iter()
            .map(|s| {
                let name = format!("DS{}", s.name.to_ascii_uppercase());
                let samples = if let Some(lump) = wad.get_lump(&name) {
                    lump_sfx_to_f32(&lump.data).unwrap_or_else(|| {
                        warn!("{name} failed to parse");
                        Vec::new()
                    })
                } else {
                    debug!("{name} is missing");
                    Vec::new()
                };
                SfxChunk {
                    samples,
                    priority: s.priority,
                }
            })
            .collect();
        info!("Initialised {} sfx (rodio)", chunks.len());

        // Convert MUS→MIDI for all music tracks
        let mut midi_data = Vec::new();
        let mut mus_count = 0;
        unsafe {
            #[allow(static_mut_refs)]
            for mus in MUS_DATA.iter_mut() {
                if let Some(lump) = wad.get_lump(mus.lump_name().as_str()) {
                    if lump.data[..4] == MUS_ID {
                        if let Some(res) = read_mus_to_midi(&lump.data) {
                            midi_data.push(res);
                            mus_count += 1;
                            continue;
                        }
                    } else if lump.data[..4] == MID_ID {
                        midi_data.push(lump.data.clone());
                        mus_count += 1;
                        continue;
                    }
                }
                midi_data.push(Vec::new());
            }
        }
        info!("Initialised {mus_count} midi songs (rodio)");

        let opl_state = Arc::new(Mutex::new(OplPlayerState::new(SAMPLE_RATE, wad)));

        let (tx, rx) = channel();
        let mixer = Arc::new(Mutex::new(DoomMixer::new()));

        Ok(Self {
            rx,
            tx,
            mixer,
            opl_state,
            stream: None,
            chunks,
            midi_data,
            listener: SoundObject::default(),
            sources: [SoundObject::default(); MIXER_CHANNELS as usize],
            sfx_vol: 64,
            mus_vol: 64,
        })
    }

    /// Create the audio output stream. Must be called on the sound thread
    /// because the sink is `!Send`.
    fn init_stream(&mut self) -> Result<(), SndError> {
        let sink =
            DeviceSinkBuilder::open_default_sink().map_err(|e| SndError::Init(e.to_string()))?;

        let mixer_source = SharedMixerSource {
            mixer: Arc::clone(&self.mixer),
        };
        sink.mixer().add(mixer_source);

        let opl_source = OplSource::new(Arc::clone(&self.opl_state));
        sink.mixer().add(opl_source);

        self.stream = Some(StreamState {
            _sink: sink,
        });
        info!("Audio output stream initialised (rodio/cpal)");
        Ok(())
    }

    fn dist_scale(dist: f32) -> f32 {
        (1.0 - dist / MAX_DIST).clamp(0.0, 1.0)
    }

    fn angle_to_pan(angle_deg: f32) -> f32 {
        // SDL2 convention: 0=front, 90=right, 180=back, 270=left
        // Convert to pan: sin gives right-positive
        let rad = angle_deg.to_radians();
        rad.sin()
    }
}

/// Wrapper that reads from the shared mixer behind an `Arc<Mutex<>>`
struct SharedMixerSource {
    mixer: Arc<Mutex<DoomMixer>>,
}

impl Iterator for SharedMixerSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if let Ok(mut m) = self.mixer.lock() {
            m.next()
        } else {
            Some(0.0)
        }
    }
}

impl Source for SharedMixerSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        rodio::ChannelCount::new(2).unwrap()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        rodio::SampleRate::new(SAMPLE_RATE).unwrap()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

impl SoundServer<SfxName, usize, SndError> for Snd {
    fn init(&mut self) -> InitResult<SfxName, usize, SndError> {
        self.init_stream()?;
        Ok(self.tx.clone())
    }

    fn start_sound(&mut self, uid: usize, sfx: SfxName, mut x: f32, mut y: f32) {
        if uid == 0 {
            x = self.listener.x;
            y = self.listener.y;
        }

        let dist = dist_from_points(self.listener.x, self.listener.y, x, y);
        if dist >= MAX_DIST {
            return;
        }

        let distance_vol = Self::dist_scale(dist);
        let pan = if uid != self.listener.uid && uid != 0 {
            let angle = listener_to_source_angle_deg(
                self.listener.x,
                self.listener.y,
                self.listener.angle,
                x,
                y,
            );
            Self::angle_to_pan(angle)
        } else {
            0.0
        };

        // Stop any existing sound from this source
        self.stop_sound(uid);

        let chunk = &self.chunks[sfx as usize];
        if chunk.samples.is_empty() {
            return;
        }

        let origin = SoundObject {
            uid,
            sfx,
            x,
            y,
            angle: 0.0,
            channel: 0,
            priority: chunk.priority,
        };

        if let Ok(mut mixer) = self.mixer.lock() {
            // Find a free channel
            let mut assigned = false;
            for c in 0..MIXER_CHANNELS as usize {
                if !mixer.channels[c].active {
                    mixer.channels[c] = ChannelState {
                        samples: chunk.samples.clone(),
                        cursor: 0,
                        active: true,
                        priority: chunk.priority,
                        pan,
                        distance_vol,
                    };
                    let mut o = origin;
                    o.channel = c as i32;
                    self.sources[c] = o;
                    assigned = true;
                    break;
                }
            }

            // Priority eviction
            if !assigned {
                for c in 0..MIXER_CHANNELS as usize {
                    if origin.priority >= mixer.channels[c].priority {
                        mixer.channels[c] = ChannelState {
                            samples: chunk.samples.clone(),
                            cursor: 0,
                            active: true,
                            priority: chunk.priority,
                            pan,
                            distance_vol,
                        };
                        let mut o = origin;
                        o.channel = c as i32;
                        self.sources[c] = o;
                        break;
                    }
                }
            }
        }
    }

    fn update_listener(&mut self, uid: usize, x: f32, y: f32, angle: f32) {
        self.listener.uid = uid;
        self.listener.x = x;
        self.listener.y = y;
        self.listener.angle = angle;

        if let Ok(mut mixer) = self.mixer.lock() {
            for (i, s) in self.sources.iter().enumerate() {
                if s.uid != 0 && mixer.channels[i].active {
                    let dist = dist_from_points(self.listener.x, self.listener.y, s.x, s.y);
                    if dist >= MAX_DIST {
                        mixer.channels[i].active = false;
                        continue;
                    }

                    mixer.channels[i].distance_vol = Self::dist_scale(dist);
                    if s.uid != self.listener.uid {
                        let angle_deg = listener_to_source_angle_deg(
                            self.listener.x,
                            self.listener.y,
                            self.listener.angle,
                            s.x,
                            s.y,
                        );
                        mixer.channels[i].pan = Self::angle_to_pan(angle_deg);
                    }
                }
            }
        }
    }

    fn stop_sound(&mut self, uid: usize) {
        if let Ok(mut mixer) = self.mixer.lock() {
            for (i, s) in self.sources.iter_mut().enumerate() {
                if s.uid == uid {
                    mixer.channels[i].active = false;
                    *s = SoundObject::default();
                }
            }
        }
    }

    fn stop_sound_all(&mut self) {
        if let Ok(mut mixer) = self.mixer.lock() {
            for c in mixer.channels.iter_mut() {
                c.active = false;
            }
        }
        for s in self.sources.iter_mut() {
            *s = SoundObject::default();
        }
    }

    fn set_sfx_volume(&mut self, volume: i32) {
        self.sfx_vol = volume;
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.master_volume = volume as f32 / 128.0;
        }
    }

    fn get_sfx_volume(&mut self) -> i32 {
        self.sfx_vol
    }

    fn start_music(&mut self, music: usize, looping: bool) {
        if music >= self.midi_data.len() || self.midi_data[music].is_empty() {
            return;
        }
        if let Ok(mut opl) = self.opl_state.lock() {
            if let Err(e) = opl.load_music(self.midi_data[music].clone()) {
                warn!("Failed to load OPL music: {e}");
                return;
            }
            opl.start_playback(looping);
            opl.volume = self.mus_vol.clamp(0, 128);
        }
    }

    fn pause_music(&mut self) {
        if let Ok(mut opl) = self.opl_state.lock() {
            opl.stop_playback();
        }
    }

    fn resume_music(&mut self) {
        if let Ok(mut opl) = self.opl_state.lock() {
            opl.start_playback(true);
        }
    }

    fn change_music(&mut self, music: usize, looping: bool) {
        self.stop_music();
        self.start_music(music, looping);
    }

    fn stop_music(&mut self) {
        if let Ok(mut opl) = self.opl_state.lock() {
            opl.stop_playback();
        }
    }

    fn set_mus_volume(&mut self, volume: i32) {
        self.mus_vol = volume;
        if let Ok(mut opl) = self.opl_state.lock() {
            opl.volume = volume.clamp(0, 128);
            opl.refresh_all_volumes();
        }
    }

    fn get_mus_volume(&mut self) -> i32 {
        self.mus_vol
    }

    fn update_self(&mut self) {}

    fn get_rx(&mut self) -> &mut SndServerRx {
        &mut self.rx
    }

    fn shutdown_sound(&mut self) {
        info!("Shutdown sound server (rodio)");
        self.stop_sound_all();
        self.stop_music();
        self.stream.take();
    }
}

impl SoundServerTic<SfxName, usize, SndError> for Snd {}
