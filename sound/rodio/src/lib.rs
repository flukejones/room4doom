//! Pure-Rust sound backend using rodio (cpal) for audio output.
//!
//! Replaces SDL2_mixer for SFX playback. Music support (OPL/rustysynth)
//! is handled by separate source modules added to the same output stream.

use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use log::{debug, info, warn};
use opl2_emulator::OplPlayerState;
use rodio::{DeviceSinkBuilder, MixerDeviceSink};
use sound_common::{
    MAX_DIST, MIXER_CHANNELS, MusicType, SAMPLE_RATE, SFX_INFO_BASE, SfxName, SndServerRx,
    SndServerTx, SoundAction, SoundObject, dist_from_points, listener_to_source_angle_deg,
};
use wad::WadData;

/// Channel poll timeout per `tic`. Short enough that shutdown latency is
/// human-imperceptible, long enough that the sound thread doesn't busy-spin.
const TIC_POLL_TIMEOUT: Duration = Duration::from_micros(500);

#[macro_use]
mod source_format;

mod mixer;
use mixer::{BUFFER_SAMPLES, ChannelState, SfxMixer};

mod opl_source;
use opl_source::OplSource;

mod gus_source;
use gus_source::{GusPlayerState, GusSource};

/// Pre-loaded sound effect data
struct SfxChunk {
    /// Mono f32 samples at 44100 Hz. `Arc` so playing a sound shares the
    /// buffer with the channel instead of copying it.
    samples: Arc<[f32]>,
    /// Playback priority
    priority: i32,
}

/// Holds the live cpal output handle for the duration of the server's
/// run. Dropping `StreamState` closes the audio stream. `MixerDeviceSink`
/// is `!Send`, which is what makes `Snd` itself `!Send`.
struct StreamState {
    _sink: MixerDeviceSink,
}

/// `Send`-able sound server configuration. Carries everything needed to
/// build a [`Snd`] *except* the audio output sink, which is opened later
/// on the sound thread itself (the sink is `!Send`).
///
/// All sfx and music asset loading happens here on the caller's thread,
/// so the audio thread never blocks on file I/O during startup.
pub struct SndConfig {
    chunks: Vec<SfxChunk>,
    opl_state: Arc<Mutex<OplPlayerState>>,
    gus_state: Option<Arc<Mutex<GusPlayerState>>>,
    music_type: MusicType,
}

impl SndConfig {
    /// Build the configuration from a WAD and user-supplied options.
    /// Loads all sfx lumps, constructs the OPL synthesizer state, and
    /// (optionally) loads the GUS SoundFont. All failures degrade
    /// gracefully — a missing/malformed sfx becomes silent, a failed
    /// SF2 falls back to OPL music.
    pub fn from_wad(
        wad: &WadData,
        music_type: MusicType,
        sf2_path: Option<&std::path::Path>,
    ) -> Self {
        let chunks = load_sfx_chunks(wad);
        let opl_state = Arc::new(Mutex::new(OplPlayerState::new(SAMPLE_RATE, wad)));
        // Always try to load the SF2 so GUS is available for runtime switching.
        let gus_state = load_gus_state(sf2_path);
        let active_type = resolve_music_type(music_type, gus_state.is_some());
        Self {
            chunks,
            opl_state,
            gus_state,
            music_type: active_type,
        }
    }
}

/// Pure-Rust sound server using rodio.
///
/// `Snd` is `!Send` because `StreamState` holds a `MixerDeviceSink` that
/// owns a cpal device handle; cpal's portability story does not promise
/// device handles can cross threads on every platform. The server is
/// constructed and consumed entirely on the sound thread via [`spawn`]
/// — see that function for the construction flow. The type is
/// `pub(crate)` because the only external-facing entry points are
/// [`SndConfig::from_wad`] and [`spawn`].
pub(crate) struct Snd {
    rx: SndServerRx,
    mixer: Arc<Mutex<SfxMixer>>,
    opl_state: Arc<Mutex<OplPlayerState>>,
    gus_state: Option<Arc<Mutex<GusPlayerState>>>,
    music_type: MusicType,
    stream: Option<StreamState>,
    chunks: Vec<SfxChunk>,
    listener: SoundObject<SfxName>,
    sources: [SoundObject<SfxName>; MIXER_CHANNELS as usize],
    sfx_vol: i32,
    mus_vol: i32,
}

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

/// Load all sfx lumps from the WAD into in-memory `SfxChunk`s, parallel
/// to the order of `SFX_INFO_BASE`. Missing or malformed lumps degrade
/// to empty samples (logged but never fatal); index-stability with
/// `SfxName as usize` is preserved.
fn load_sfx_chunks(wad: &WadData) -> Vec<SfxChunk> {
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
                samples: samples.into(),
                priority: s.priority,
            }
        })
        .collect();
    info!("Initialised {} sfx (rodio)", chunks.len());
    chunks
}

/// Load the GUS SoundFont if a path was supplied. Logs at info on
/// success, warn on parse failure, and returns `None` for either
/// "no path provided" or "load failed" — the caller falls back to
/// OPL music in either case.
fn load_gus_state(sf2_path: Option<&std::path::Path>) -> Option<Arc<Mutex<GusPlayerState>>> {
    let path = sf2_path?;
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
}

/// Choose the active music type given the user-requested type and
/// whether the GUS SoundFont was actually loaded. GUS without an SF2
/// has no synthesizer to drive, so silently fall back to OPL2.
fn resolve_music_type(requested: MusicType, gus_loaded: bool) -> MusicType {
    if requested == MusicType::GUS && !gus_loaded {
        MusicType::OPL2
    } else {
        requested
    }
}

/// Spawn the rodio sound server on a dedicated thread.
///
/// Returns the `Sender` end of the action channel (for the caller to
/// push `SoundAction`s) and a `JoinHandle` for the spawned thread. The
/// server runs until it receives `SoundAction::Shutdown` or the
/// `Sender` is dropped (channel closes).
///
/// Construction of `Snd` itself — including opening the cpal audio
/// device and wiring the music sources into the rodio sink — happens
/// entirely on the spawned thread. This keeps the `!Send`
/// `MixerDeviceSink` from ever crossing a thread boundary, eliminating
/// the previous `unsafe impl Send for Snd` overpromise.
pub fn spawn(config: SndConfig) -> (SndServerTx, JoinHandle<()>) {
    let (tx, rx) = channel();
    let handle = thread::spawn(move || {
        let mut snd = Snd::start(config, rx);
        while snd.tic() {}
    });
    (tx, handle)
}

impl Snd {
    /// Construct the server on the sound thread and open the audio
    /// output stream. Private — the only call site is `spawn`. Building
    /// `Snd` directly is not exposed because the sink is `!Send` and we
    /// want to enforce thread-locality at the API boundary.
    fn start(config: SndConfig, rx: SndServerRx) -> Self {
        let mixer = Arc::new(Mutex::new(SfxMixer::new()));
        let mut snd = Self {
            rx,
            mixer,
            opl_state: config.opl_state,
            gus_state: config.gus_state,
            music_type: config.music_type,
            stream: None,
            chunks: config.chunks,
            listener: SoundObject::default(),
            sources: [SoundObject::default(); MIXER_CHANNELS as usize],
            sfx_vol: 64,
            mus_vol: 64,
        };
        snd.init_stream();
        snd
    }

    /// Open the default audio output device and wire the mixer + music
    /// sources into it. Called once by `start` on construction.
    ///
    /// On failure (no device, busy device, unsupported format)
    /// `self.stream` stays `None` and the server runs in silent mode for
    /// the lifetime of this run — `tic` still drains the action channel
    /// and internal state still tracks volume/listener/sources, so a
    /// future event-driven reconnect (see TODO.md "Sound") can resume
    /// playback without restart. The user must restart the engine to
    /// pick up a new audio device until that lands.
    fn init_stream(&mut self) {
        let sink = match DeviceSinkBuilder::open_default_sink() {
            Ok(s) => s,
            Err(e) => {
                warn!("No audio output device available: {e}. Running silent.");
                return;
            }
        };

        let mixer_source = SfxMixerSource::new(Arc::clone(&self.mixer));
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
        info!(
            "Audio output stream initialised (rodio/cpal, music: {:?})",
            self.music_type
        );
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

/// rodio `Source` adapter that pulls from the shared `SfxMixer`.
///
/// The mixer lives behind `Arc<Mutex<>>` because the audio callback
/// thread (this `next()`) and the sound thread (sound-action handlers)
/// both need access; this struct exists so rodio's `Source` trait sees
/// a `!Mutex` type while internally we synchronise on each pull.
struct SfxMixerSource {
    mixer: Arc<Mutex<SfxMixer>>,
    /// Staging copy of the last mixed block, dispensed lock-free per sample.
    block: Vec<f32>,
    /// Cursor into `block`; `>= block.len()` triggers a locked refill.
    pos: usize,
}

impl SfxMixerSource {
    fn new(mixer: Arc<Mutex<SfxMixer>>) -> Self {
        Self {
            mixer,
            block: vec![0.0; BUFFER_SAMPLES],
            pos: BUFFER_SAMPLES, // start exhausted: first next() triggers a fill
        }
    }
}

impl Iterator for SfxMixerSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.pos >= self.block.len() {
            // One lock per block, not per sample. On poison, emit a silent
            // block rather than panicking on the audio callback thread;
            // warning is logged inside `with_lock`.
            if with_lock(&self.mixer, "sfx mixer", |m| m.fill_block(&mut self.block)).is_none() {
                self.block.fill(0.0);
            }
            self.pos = 0;
        }
        let sample = self.block[self.pos];
        self.pos += 1;
        Some(sample)
    }
}

impl_stereo_source!(SfxMixerSource);

impl Snd {
    /// Drain at most one queued `SoundAction` from the producer channel
    /// and dispatch it. Returns `false` only on `Shutdown`, signalling
    /// the sound thread loop to exit.
    fn tic(&mut self) -> bool {
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
            let new_state = || ChannelState {
                samples: Arc::clone(&chunk.samples),
                cursor: 0,
                active: true,
                priority: chunk.priority,
                pan,
                distance_vol,
            };

            // Find a free channel
            let assigned = mixer.channels.iter().position(|ch| !ch.active).or_else(|| {
                // Priority eviction
                mixer
                    .channels
                    .iter()
                    .position(|ch| origin.priority >= ch.priority)
            });

            if let Some(c) = assigned {
                mixer.channels[c] = new_state();
                let mut o = origin;
                o.channel = c as i32;
                sources[c] = o;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_music_type_gus_without_sf2_falls_back_to_opl2() {
        assert_eq!(resolve_music_type(MusicType::GUS, false), MusicType::OPL2);
    }

    #[test]
    fn resolve_music_type_gus_with_sf2_stays_gus() {
        assert_eq!(resolve_music_type(MusicType::GUS, true), MusicType::GUS);
    }

    #[test]
    fn resolve_music_type_opl_passes_through() {
        assert_eq!(resolve_music_type(MusicType::OPL2, false), MusicType::OPL2);
        assert_eq!(resolve_music_type(MusicType::OPL2, true), MusicType::OPL2);
        assert_eq!(resolve_music_type(MusicType::OPL3, false), MusicType::OPL3);
        assert_eq!(resolve_music_type(MusicType::OPL3, true), MusicType::OPL3);
    }
}
