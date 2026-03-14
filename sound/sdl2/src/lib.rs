//! SDL2 Sound Backend for Room4Doom
//!
//! This module provides SDL2-based audio playback for sound effects and music.
//! It supports multiple music backends:
//!
//! - **SDL_mixer**: Traditional MIDI playback using TiMidity or FluidSynth
//! - **OPL2 Emulator**: Authentic FM synthesis using the integrated OPL2
//!   emulator
//!
//! ## OPL2 Support
//!
//! The OPL2 emulator provides authentic Yamaha YM3812 FM synthesis for music
//! playback. This gives the most accurate reproduction of the original Doom
//! music as it was intended to be heard on AdLib and Sound Blaster cards.
//!
//! ### Configuration
//!
//! Set the music type to OPL2 in your configuration:
//! ```toml
//! music_type = "OPL2"
//! ```
//!
//! Or use the command line:
//! ```bash
//! ./doom --music-type opl2
//! ```
//!
//! ### Features
//!
//! - Authentic FM synthesis using OPL2 emulation
//! - Support for MUS format music files
//! - Real-time music playback with proper timing
//! - Volume control and pause/resume functionality
//! - Automatic fallback to SDL_mixer if OPL2 initialization fails

use std::error::Error;
use std::fmt::Debug;
use std::sync::mpsc::channel;

use log::{debug, info, warn};
use sdl2::AudioSubsystem;
use sdl2::audio::{AudioCVT, AudioFormat};
use sdl2::mixer::{AUDIO_S16LSB, Chunk, DEFAULT_CHANNELS, InitFlag, Music, Sdl2MixerContext};
use sound_common::{
    InitResult, MAX_DIST, MID_ID, MIXER_CHANNELS, MUS_DATA, MUS_ID, SFX_INFO_BASE, SfxName, SndServerRx, SndServerTx, SoundObject, SoundServer, SoundServerTic, listener_to_source_angle_deg, read_mus_to_midi
};
use wad::WadData;

use crate::opl2::OplPlayer;

pub use sound_common::{dist_from_points, point_to_angle_2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MusicType {
    Timidity,
    FluidSynth,
    OPL2,
    OPL3,
}

pub mod mus2midi;
pub mod opl2;
pub mod timidity;

#[cfg(test)]
mod test_sdl2;

struct SfxInfo {
    /// Up to 6-character name. In the Lump the names are typically prefixed by
    /// `DS` or `DP`, so the full Lump name is 8-char, while the name here
    /// has the prefix striped off.
    _name: String,
    /// Priority of sound
    priority: i32,

    // Not really used
    _pitch: i32,
    _volume: i32,

    /// Pre-processed SDL2 Chunk data
    data: Option<Chunk>,
    /// this is checked every second to see if sound can be thrown out (if 0,
    /// then decrement, if -1, then throw out, if > 0, then it is in use)
    _usefulness: i32,
}

impl SfxInfo {
    pub(crate) fn new(name: String, priority: i32, data: Option<Chunk>) -> Self {
        Self {
            _name: name,
            priority,
            _pitch: -1,
            _volume: -1,
            data,
            _usefulness: 0,
        }
    }
}

/// `to_fmt` is almost always going to be `AudioFormat::S16LSB`, `to_rate`
/// typically `44_100`.
fn lump_sfx_to_chunk(
    raw_lump: Vec<u8>,
    to_fmt: AudioFormat,
    to_rate: i32,
) -> Result<Chunk, String> {
    let mut rate = i16::from_le_bytes([raw_lump[2], raw_lump[3]]) as i32;
    if rate <= 0 {
        rate = to_rate;
    }
    let len = i32::from_le_bytes([raw_lump[4], raw_lump[5], raw_lump[6], raw_lump[7]]);
    let converter = AudioCVT::new(AudioFormat::U8, 1, rate, to_fmt, 2, to_rate)?;
    let fixed = converter.convert(raw_lump[7..len as usize].to_vec());

    Chunk::from_raw_buffer(fixed.into_boxed_slice()).map(|mut c| {
        // Set base volume
        c.set_volume(128);
        c
    })
}

pub struct Snd<'a> {
    _audio: AudioSubsystem,
    _mixer: Sdl2MixerContext,
    rx: SndServerRx,
    tx: SndServerTx,
    chunks: Vec<SfxInfo>,
    music: Option<Music<'a>>,
    opl_player: Option<OplPlayer>,
    use_opl2: bool,
    listener: SoundObject<SfxName>,
    sources: [SoundObject<SfxName>; MIXER_CHANNELS as usize],
    sfx_vol: i32,
    mus_vol: i32,
}

unsafe impl<'a> Send for Snd<'a> {}

impl<'a> Snd<'a> {
    pub fn new(
        audio: AudioSubsystem,
        wad: &WadData,
        music_type: MusicType,
    ) -> Result<Self, Box<dyn Error>> {
        // let mut timer = sdl.timer()?;
        let frequency = 44_100;
        let format = AUDIO_S16LSB; // signed 16 bit samples, in little-endian byte order
        let channels = DEFAULT_CHANNELS; // Stereo
        let chunk_size = 1_024;

        sdl2::mixer::open_audio(frequency, format, channels, chunk_size)?;
        let _mixer = if let Ok(m) = sdl2::mixer::init(InitFlag::MID | InitFlag::MP3 | InitFlag::OGG)
        {
            m
        } else if let Ok(m) = sdl2::mixer::init(InitFlag::MID | InitFlag::OGG) {
            m
        } else {
            sdl2::mixer::init(InitFlag::MID)?
        };
        // Mixer channels are not play/stereo channels
        sdl2::mixer::allocate_channels(MIXER_CHANNELS);

        info!("Using sound driver: {}", audio.current_audio_driver());

        let chunks: Vec<SfxInfo> = SFX_INFO_BASE
            .iter()
            .map(|s| {
                let name = format!("DS{}", s.name.to_ascii_uppercase());
                if let Some(lump) = wad.get_lump(&name) {
                    let chunk = lump_sfx_to_chunk(lump.data.clone(), AudioFormat::S16LSB, 44_100)
                        .unwrap_or_else(|_| panic!("{name} failed to parse"));
                    SfxInfo::new(s.name.to_string(), s.priority, Some(chunk))
                } else {
                    debug!("{name} is missing");
                    SfxInfo::new(s.name.to_string(), s.priority, None)
                }
            })
            .collect();
        info!("Initialised {} sfx", chunks.len());

        let mut mus_count = 0;
        unsafe {
            // TODO: make function unsafe to call instead to reflect the static mut
            #[allow(static_mut_refs)]
            for mus in MUS_DATA.iter_mut() {
                if let Some(lump) = wad.get_lump(mus.lump_name().as_str()) {
                    if lump.data[..4] == MUS_ID {
                        if let Some(res) = read_mus_to_midi(&lump.data) {
                            mus.set_data(res);
                            mus_count += 1;
                        }
                    } else if lump.data[..4] == MID_ID {
                        // It's MIDI
                        mus.set_data(lump.data.clone());
                        mus_count += 1;
                    }
                } else {
                    debug!("{} is missing", mus.lump_name().as_str());
                }
            }
        }
        info!("Initialised {} midi songs", mus_count);

        // Initialize OPL2 player if requested
        let mut use_opl2 = false;
        let opl_player = if matches!(music_type, MusicType::OPL2 | MusicType::OPL3) {
            match OplPlayer::new(&audio, wad) {
                Ok(player) => {
                    info!("{music_type:?} music player initialized");
                    use_opl2 = true;
                    Some(player)
                }
                Err(e) => {
                    warn!(
                        "Failed to initialize {music_type:?} player: {e}, falling back to SDL_mixer",
                    );
                    None
                }
            }
        } else {
            None
        };

        let (tx, rx) = channel();
        Ok(Self {
            _audio: audio,
            _mixer,
            rx,
            tx,
            chunks,
            music: None,
            opl_player,
            use_opl2,
            listener: SoundObject::default(),
            sources: [SoundObject::default(); MIXER_CHANNELS as usize],
            sfx_vol: 64,
            mus_vol: 64,
        })
    }

    fn listener_to_source_angle(&self, sx: f32, sy: f32) -> f32 {
        listener_to_source_angle_deg(
            self.listener.x,
            self.listener.y,
            self.listener.angle,
            sx,
            sy,
        )
    }

    fn dist_from_listener(&self, sx: f32, sy: f32) -> f32 {
        dist_from_points(self.listener.x, self.listener.y, sx, sy)
    }

    fn dist_scale_sdl2(dist: f32) -> f32 {
        dist * 255.0 / MAX_DIST
    }
}

impl<'a> SoundServer<SfxName, usize, sdl2::Error> for Snd<'a> {
    fn init(&mut self) -> InitResult<SfxName, usize, sdl2::Error> {
        Ok(self.tx.clone())
    }

    fn start_sound(&mut self, uid: usize, sfx: SfxName, mut x: f32, mut y: f32) {
        if uid == 0 {
            x = self.listener.x;
            y = self.listener.y;
        }
        let mut dist = self.dist_from_listener(x, y);
        if dist >= MAX_DIST {
            // Not audible
            return;
        }
        // Scale for SDL2
        dist = Self::dist_scale_sdl2(dist);
        let mut angle = 0.0;
        if uid != self.listener.uid && uid != 0 {
            angle = self.listener_to_source_angle(x, y);
        }

        // Stop any existing sound this source is emitting
        self.stop_sound(uid);

        let chunk = &self.chunks[sfx as usize];
        let mut origin = SoundObject {
            uid,
            sfx,
            x,
            y,
            angle,
            channel: 0,
            priority: chunk.priority,
        };

        if let Some(sfx) = chunk.data.as_ref() {
            let mut playing = false;
            for c in 0..MIXER_CHANNELS {
                if !sdl2::mixer::Channel(c).is_playing() || sdl2::mixer::Channel(c).is_paused() {
                    if origin.uid != self.listener.uid {
                        sdl2::mixer::Channel(c)
                            .set_position(angle as i16, dist as u8)
                            .unwrap();
                    }
                    sdl2::mixer::Channel(c).play(sfx, 0).unwrap();
                    origin.channel = c;
                    self.sources[c as usize] = origin;
                    playing = true;
                    break;
                }
            }
            // evict a sound maybe
            if !playing {
                for c in 0..MIXER_CHANNELS {
                    if origin.priority >= self.sources[c as usize].priority {
                        sdl2::mixer::Channel(c).halt();
                        if origin.uid != self.listener.uid {
                            sdl2::mixer::Channel(c)
                                .set_position(angle as i16, dist as u8)
                                .unwrap();
                        }
                        sdl2::mixer::Channel(c).play(sfx, 0).unwrap();
                        origin.channel = c;
                        self.sources[c as usize] = origin;
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

        for s in self.sources.iter() {
            if s.uid != 0 && sdl2::mixer::Channel(s.channel).is_playing() {
                let mut dist = self.dist_from_listener(s.x, s.y);
                // Is it too far away now?
                if dist >= MAX_DIST {
                    sdl2::mixer::Channel(s.channel).halt();
                    continue;
                }
                // Scale for SDL2
                dist = Self::dist_scale_sdl2(dist);

                let mut angle = 0.0;
                if s.uid != self.listener.uid {
                    angle = self.listener_to_source_angle(s.x, s.y);
                }

                sdl2::mixer::Channel(s.channel)
                    .set_position(angle as i16, dist as u8)
                    .unwrap();
            }
        }
    }

    fn stop_sound(&mut self, uid: usize) {
        for s in self.sources.iter_mut() {
            if s.uid == uid {
                sdl2::mixer::Channel(s.channel).halt();
                *s = SoundObject::default();
            }
        }
    }

    fn stop_sound_all(&mut self) {
        for s in self.sources.iter_mut() {
            *s = SoundObject::default();
        }
        for c in 0..MIXER_CHANNELS {
            sdl2::mixer::Channel(c).halt()
        }
    }

    fn set_sfx_volume(&mut self, volume: i32) {
        self.sfx_vol = volume;
        sdl2::mixer::Channel::all().set_volume(self.sfx_vol);
    }

    fn get_sfx_volume(&mut self) -> i32 {
        self.sfx_vol
    }

    fn start_music(&mut self, music: usize, looping: bool) {
        // Use OPL2 if configured and available
        if self.use_opl2 && self.opl_player.is_some() {
            unsafe {
                let music_data = MUS_DATA[music].data();
                if let Some(ref mut opl) = self.opl_player {
                    if let Err(e) = opl.load_music(music_data.to_vec()) {
                        log::error!("Failed to load OPL2 music: {}", e);
                    } else if let Err(e) = opl.play(looping) {
                        log::error!("Failed to play OPL2 music: {}", e);
                    } else {
                        opl.set_volume(self.mus_vol);
                        debug!("Playing {} with OPL2", MUS_DATA[music].lump_name());
                        return;
                    }
                }
            }
        }

        // Fall back to SDL_mixer
        unsafe {
            if let Ok(music) = Music::from_static_bytes(MUS_DATA[music].data())
                .map_err(|e| log::error!("MUS: {}, error: {e}", MUS_DATA[music].lump_name()))
            {
                music.play(if looping { -1 } else { 0 }).unwrap();
                self.music = Some(music);
                Music::set_volume(self.mus_vol);
                debug!("Playing music with SDL_mixer");
            }
        }
    }

    fn pause_music(&mut self) {
        if let Some(ref mut opl) = self.opl_player
            && opl.is_playing().unwrap_or(false)
        {
            opl.pause();
            return;
        }
        Music::pause();
    }

    fn resume_music(&mut self) {
        if let Some(ref mut opl) = self.opl_player {
            opl.resume();
            // Don't return here - also resume SDL music in case both are active
        }
        Music::resume();
    }

    fn change_music(&mut self, music: usize, looping: bool) {
        if let Some(ref mut opl) = self.opl_player {
            opl.stop();
        }
        Music::halt();
        self.music.take();
        self.start_music(music, looping)
    }

    fn stop_music(&mut self) {
        if let Some(ref mut opl) = self.opl_player {
            opl.stop();
        }
        Music::halt();
    }

    fn set_mus_volume(&mut self, volume: i32) {
        self.mus_vol = volume;
        if let Some(ref mut opl) = self.opl_player {
            opl.set_volume(volume);
        }
        Music::set_volume(volume);
    }

    fn get_mus_volume(&mut self) -> i32 {
        self.mus_vol
    }

    fn update_self(&mut self) {}

    fn get_rx(&mut self) -> &mut SndServerRx {
        &mut self.rx
    }

    fn shutdown_sound(&mut self) {
        info!("Shutdown sound server");
        self.stop_sound_all();
        self.stop_music();
        if let Some(ref mut opl) = self.opl_player {
            opl.stop();
        }
    }
}

impl<'a> SoundServerTic<SfxName, usize, sdl2::Error> for Snd<'a> {}

#[cfg(test)]
mod tests {
    use crate::mus2midi::read_mus_to_midi;
    use sdl2::mixer::{AUDIO_S16LSB, DEFAULT_CHANNELS, InitFlag};
    use sound_common::MUS_DATA;
    use std::path::PathBuf;
    use std::time::Duration;
    use wad::WadData;

    fn doom1_wad_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("doom1.wad")
    }

    #[ignore = "CI doesn't have a sound device"]
    #[test]
    fn write_map_mus_data() {
        let wad = WadData::new(&doom1_wad_path());

        unsafe {
            #[allow(static_mut_refs)]
            for mus in MUS_DATA.iter_mut() {
                if let Some(lump) = wad.get_lump(mus.lump_name().as_str()) {
                    dbg!(mus.lump_name());
                    let res = read_mus_to_midi(&lump.data).unwrap();
                    mus.set_data(res);
                }
            }
        }

        let sdl = sdl2::init().unwrap();
        let _audio = sdl.audio().unwrap();

        let frequency = 44_100;
        let format = AUDIO_S16LSB; // signed 16 bit samples, in little-endian byte order
        let channels = DEFAULT_CHANNELS; // Stereo
        let chunk_size = 1_024;
        sdl2::mixer::open_audio(frequency, format, channels, chunk_size).unwrap();
        let _mixer_context = sdl2::mixer::init(InitFlag::MOD).unwrap();

        // Number of mixing channels available for sound effect `Chunk`s to play
        // simultaneously.
        sdl2::mixer::allocate_channels(16);

        let music = unsafe { sdl2::mixer::Music::from_static_bytes(MUS_DATA[1].data()).unwrap() };

        println!("music => {:?}", music);
        println!("music type => {:?}", music.get_type());
        println!("music volume => {:?}", sdl2::mixer::Music::get_volume());
        println!("play => {:?}", music.play(1));

        std::thread::sleep(Duration::from_secs(10));
    }
}
