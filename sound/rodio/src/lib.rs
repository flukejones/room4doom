//! Pure-Rust sound backend using rodio (cpal) for audio output.
//!
//! Replaces SDL2_mixer for SFX playback. Music support (OPL/rustysynth)
//! is handled by separate source modules added to the same output stream.

use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use log::{debug, info, warn};
use opl2_emulator::OplPlayerState;
use rodio::{DeviceSinkBuilder, MixerDeviceSink};
use sound_common::{
    MAX_DIST, MIXER_CHANNELS, MusicType, SAMPLE_RATE, SFX_INFO_BASE, SfxName, SndServerRx, SndServerTx, SoundAction, SoundObject, dist_from_points, listener_to_source_angle_deg
};
use wad::WadData;

/// Channel poll timeout per `tic`. Short enough that shutdown latency is
/// human-imperceptible, long enough that the sound thread doesn't busy-spin.
const TIC_POLL_TIMEOUT: Duration = Duration::from_micros(500);
/// How often `tic` re-tries opening an audio device while in silent mode.
/// Sink construction is heavyweight (allocates audio buffers, talks to OS),
/// so a coarse retry interval keeps overhead negligible while still
/// reacting within human-perceptible latency to a hot-plugged device.
const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);

#[macro_use]
mod source_format;

mod mixer;
use mixer::{ChannelState, DoomMixer};

mod opl_source;
use opl_source::OplSource;

mod gus_source;
use gus_source::{GusPlayerState, GusSource};

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
    gus_state: Option<Arc<Mutex<GusPlayerState>>>,
    music_type: MusicType,
    stream: Option<StreamState>,
    /// Last time `tic` attempted to (re)open the audio device while running
    /// silent. `None` until the first attempt; throttles retries to
    /// `RECONNECT_INTERVAL`.
    last_reconnect_attempt: Option<Instant>,
    chunks: Vec<SfxChunk>,
    listener: SoundObject<SfxName>,
    sources: [SoundObject<SfxName>; MIXER_CHANNELS as usize],
    sfx_vol: i32,
    mus_vol: i32,
}

unsafe impl Send for Snd {}

/// Acquire `mutex`, run `f` on its guard, and log a warning if the lock
/// is poisoned. Returns `f`'s result on success, `None` on poison.
/// `name` identifies the lock in the warning so a recovered log can
/// point to the source (e.g. "sfx mixer", "OPL state").
///
/// This is the project-wide pattern for "audio thread shouldn't panic
/// on a poisoned subsystem"; the side that holds the lock should be
/// panic-free anyway, so a poison here means a bug worth logging.
fn with_lock<T, R>(mutex: &Mutex<T>, name: &str, f: impl FnOnce(&mut T) -> R) -> Option<R> {
    match mutex.lock() {
        Ok(mut guard) => Some(f(&mut guard)),
        Err(e) => {
            warn!("{name} mutex poisoned: {e}");
            None
        }
    }
}

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
    /// Construct the rodio sound server. Infallible — sfx and SF2 load
    /// failures are logged and degrade to silent assets. The audio
    /// output device is opened later in `init()` and may also fall back
    /// to silent mode without erroring.
    pub fn new(
        wad: &WadData,
        music_type: MusicType,
        sf2_path: Option<&std::path::Path>,
    ) -> Self {
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

        let opl_state = Arc::new(Mutex::new(OplPlayerState::new(SAMPLE_RATE, wad)));

        // Always try to load the SF2 so GUS is available for runtime switching
        let gus_state = if let Some(path) = sf2_path {
            match GusPlayerState::new(path) {
                Ok(state) => {
                    info!("GUS SF2 loaded: {}", path.display());
                    Some(Arc::new(Mutex::new(state)))
                }
                Err(e) => {
                    warn!("Failed to load GUS SF2: {e}");
                    None
                }
            }
        } else {
            None
        };
        let active_type = if music_type == MusicType::GUS && gus_state.is_none() {
            MusicType::OPL2
        } else {
            music_type
        };

        let (tx, rx) = channel();
        let mixer = Arc::new(Mutex::new(DoomMixer::new()));

        Self {
            rx,
            tx,
            mixer,
            opl_state,
            gus_state,
            music_type: active_type,
            stream: None,
            last_reconnect_attempt: None,
            chunks,
            listener: SoundObject::default(),
            sources: [SoundObject::default(); MIXER_CHANNELS as usize],
            sfx_vol: 64,
            mus_vol: 64,
        }
    }

    /// Open the default audio output device and wire the mixer + music
    /// sources into it. Must be called on the sound thread because the
    /// sink is `!Send`. On failure (no device, busy device, unsupported
    /// format) `self.stream` stays `None`; the server then runs in
    /// silent mode — `tic` still drains the action channel and internal
    /// state still tracks volume/listener/sources, and `tic` periodically
    /// retries via `try_reconnect_silent`.
    ///
    /// `is_initial` controls log noise: the first attempt warns loudly
    /// on failure; retries log at debug level to avoid spamming.
    fn init_stream(&mut self, is_initial: bool) {
        let sink = match DeviceSinkBuilder::open_default_sink() {
            Ok(s) => s,
            Err(e) => {
                if is_initial {
                    warn!("No audio output device available: {e}. Running silent.");
                } else {
                    debug!("Audio device still unavailable: {e}");
                }
                return;
            }
        };

        let mixer_source = SharedMixerSource {
            mixer: Arc::clone(&self.mixer),
        };
        sink.mixer().add(mixer_source);

        // Add both music sources — the inactive one generates silence.
        // This allows runtime switching between music types.
        let opl_source = OplSource::new(Arc::clone(&self.opl_state));
        sink.mixer().add(opl_source);
        if let Some(ref gus) = self.gus_state {
            let gus_source = GusSource::new(Arc::clone(gus));
            sink.mixer().add(gus_source);
        }

        self.stream = Some(StreamState {
            _sink: sink,
        });
        if is_initial {
            info!(
                "Audio output stream initialised (rodio/cpal, music: {:?})",
                self.music_type
            );
        } else {
            info!("Audio output device became available — resuming sound");
        }
    }

    /// If the server is in silent mode, try once to (re)open the audio
    /// device — but no more often than `RECONNECT_INTERVAL`. Cheap when
    /// the throttle hasn't elapsed (just an `Instant` compare).
    fn try_reconnect_silent(&mut self) {
        if self.stream.is_some() {
            return;
        }
        let now = Instant::now();
        if let Some(last) = self.last_reconnect_attempt {
            if now.duration_since(last) < RECONNECT_INTERVAL {
                return;
            }
        }
        self.last_reconnect_attempt = Some(now);
        self.init_stream(false);
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
        // On poison, emit silence rather than panicking on the audio
        // callback thread; warning is logged inside `with_lock`.
        with_lock(&self.mixer, "sfx mixer", |m| m.next()).unwrap_or(Some(0.0))
    }
}

impl_stereo_source!(SharedMixerSource);

impl Snd {
    /// Initialise the audio output stream and return the sender side of
    /// the action channel. Infallible — falls back to silent mode
    /// internally if no audio device is available.
    pub fn init(&mut self) -> SndServerTx {
        self.init_stream(true);
        self.tx.clone()
    }

    /// Drain at most one queued `SoundAction` from the producer channel
    /// and dispatch it. Returns `false` only on `Shutdown`, signalling
    /// the sound thread loop to exit.
    ///
    /// Each tic also invokes `try_reconnect_silent`; this is a no-op
    /// unless the server is currently silent and the retry throttle has
    /// elapsed, so the cost is negligible in the common case.
    pub fn tic(&mut self) -> bool {
        self.try_reconnect_silent();
        let Ok(sound) = self.rx.recv_timeout(TIC_POLL_TIMEOUT) else {
            return true;
        };
        match sound {
            SoundAction::StartSfx {
                uid,
                sfx,
                x,
                y,
            } => self.start_sound(uid, sfx, x, y),
            SoundAction::UpdateListener {
                uid,
                x,
                y,
                angle,
            } => self.update_listener(uid, x, y, angle),
            SoundAction::StopSfx {
                uid,
            } => self.stop_sound(uid),
            SoundAction::StopSfxAll => self.stop_sound_all(),
            SoundAction::StartMusic(data, looping) => self.start_music(data, looping),
            SoundAction::PauseMusic => self.pause_music(),
            SoundAction::ResumeMusic => self.resume_music(),
            SoundAction::ChangeMusic(data, looping) => self.change_music(data, looping),
            SoundAction::StopMusic => self.stop_music(),
            SoundAction::SetMusicType(t) => self.set_music_type(t),
            SoundAction::SfxVolume(v) => self.set_sfx_volume(v),
            SoundAction::MusicVolume(v) => self.set_mus_volume(v),
            SoundAction::Shutdown => {
                self.shutdown_sound();
                return false;
            }
        }
        true
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

        let sources = &mut self.sources;
        with_lock(&self.mixer, "sfx mixer", |mixer| {
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
                    sources[c] = o;
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
                        sources[c] = o;
                        break;
                    }
                }
            }
        });
    }

    fn update_listener(&mut self, uid: usize, x: f32, y: f32, angle: f32) {
        self.listener.uid = uid;
        self.listener.x = x;
        self.listener.y = y;
        self.listener.angle = angle;

        let listener = self.listener;
        let sources = &self.sources;
        with_lock(&self.mixer, "sfx mixer", |mixer| {
            for (i, s) in sources.iter().enumerate() {
                if s.uid != 0 && mixer.channels[i].active {
                    let dist = dist_from_points(listener.x, listener.y, s.x, s.y);
                    if dist >= MAX_DIST {
                        mixer.channels[i].active = false;
                        continue;
                    }

                    mixer.channels[i].distance_vol = Self::dist_scale(dist);
                    if s.uid != listener.uid {
                        let angle_deg = listener_to_source_angle_deg(
                            listener.x,
                            listener.y,
                            listener.angle,
                            s.x,
                            s.y,
                        );
                        mixer.channels[i].pan = Self::angle_to_pan(angle_deg);
                    }
                }
            }
        });
    }

    fn stop_sound(&mut self, uid: usize) {
        let sources = &mut self.sources;
        with_lock(&self.mixer, "sfx mixer", |mixer| {
            for (i, s) in sources.iter_mut().enumerate() {
                if s.uid == uid {
                    mixer.channels[i].active = false;
                    *s = SoundObject::default();
                }
            }
        });
    }

    fn stop_sound_all(&mut self) {
        with_lock(&self.mixer, "sfx mixer", |mixer| {
            for c in mixer.channels.iter_mut() {
                c.active = false;
            }
        });
        for s in self.sources.iter_mut() {
            *s = SoundObject::default();
        }
    }

    fn set_sfx_volume(&mut self, volume: i32) {
        self.sfx_vol = volume;
        with_lock(&self.mixer, "sfx mixer", |mixer| {
            mixer.master_volume = volume as f32 / 128.0;
        });
    }

    fn start_music(&mut self, data: Vec<u8>, looping: bool) {
        if data.is_empty() {
            return;
        }
        let vol = self.mus_vol.clamp(0, 128);
        match self.music_type {
            MusicType::GUS => {
                if let Some(ref gus) = self.gus_state {
                    with_lock(gus, "GUS state", |g| {
                        if let Err(e) = g.load_music(&data, looping) {
                            warn!("Failed to load GUS music: {e}");
                            return;
                        }
                        g.volume = vol;
                    });
                }
            }
            MusicType::OPL2 | MusicType::OPL3 => {
                with_lock(&self.opl_state, "OPL state", |opl| {
                    if let Err(e) = opl.load_music(&data) {
                        warn!("Failed to load OPL music: {e}");
                        return;
                    }
                    opl.start_playback(looping);
                    opl.volume = vol;
                });
            }
        }
    }

    fn pause_music(&mut self) {
        match self.music_type {
            MusicType::GUS => {
                if let Some(ref gus) = self.gus_state {
                    with_lock(gus, "GUS state", |g| g.stop_playback());
                }
            }
            MusicType::OPL2 | MusicType::OPL3 => {
                with_lock(&self.opl_state, "OPL state", |opl| opl.stop_playback());
            }
        }
    }

    fn resume_music(&mut self) {
        match self.music_type {
            MusicType::GUS => {
                if let Some(ref gus) = self.gus_state {
                    with_lock(gus, "GUS state", |g| g.start_playback(true));
                }
            }
            MusicType::OPL2 | MusicType::OPL3 => {
                with_lock(&self.opl_state, "OPL state", |opl| opl.start_playback(true));
            }
        }
    }

    fn change_music(&mut self, data: Vec<u8>, looping: bool) {
        self.stop_music();
        self.start_music(data, looping);
    }

    fn stop_music(&mut self) {
        // Stop both backends to silence any previously active source
        with_lock(&self.opl_state, "OPL state", |opl| opl.stop_playback());
        if let Some(ref gus) = self.gus_state {
            with_lock(gus, "GUS state", |g| g.stop_playback());
        }
    }

    fn set_mus_volume(&mut self, volume: i32) {
        self.mus_vol = volume;
        let clamped = volume.clamp(0, 128);
        match self.music_type {
            MusicType::GUS => {
                if let Some(ref gus) = self.gus_state {
                    with_lock(gus, "GUS state", |g| g.volume = clamped);
                }
            }
            MusicType::OPL2 | MusicType::OPL3 => {
                with_lock(&self.opl_state, "OPL state", |opl| {
                    opl.volume = clamped;
                    opl.refresh_all_volumes();
                });
            }
        }
    }

    fn set_music_type(&mut self, music_type: MusicType) {
        self.music_type = music_type;
    }

    fn shutdown_sound(&mut self) {
        info!("Shutdown sound server (rodio)");
        self.stop_sound_all();
        self.stop_music();
        self.stream.take();
    }
}
