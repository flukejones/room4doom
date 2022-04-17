use std::{
    error::Error,
    fmt::Debug,
    sync::{
        atomic::AtomicBool,
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
};

use log::{info, warn};
use sdl2::{
    audio::{AudioCVT, AudioFormat},
    mixer::{Chunk, InitFlag, Sdl2MixerContext, AUDIO_S16LSB, DEFAULT_CHANNELS},
    AudioSubsystem,
};
use sound_traits::{InitResult, SfxEnum, SoundAction, SoundServer, SoundServerTic};
use wad::WadData;

use crate::info::SFX_INFO_BASE;

mod info;
pub mod music;

#[cfg(test)]
mod test_sdl2;

const MAX_DIST: f32 = 1666.0;
const MIXER_CHANNELS: i32 = 16;

pub type SndServerRx = Receiver<SoundAction<SfxEnum, i32>>;
pub type SndServerTx = Sender<SoundAction<SfxEnum, i32>>;

pub fn point_to_angle_2(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let x = x1 - x2;
    let y = y1 - y2;
    y.atan2(x)
}

#[derive(Debug, Default, Clone, Copy)]
struct SoundObject<S>
where
    S: Copy + Debug,
{
    /// Objects unique ID or hash. This should be used to track which
    /// object owns which sounds so it can be stopped e.g, death, shoot..
    uid: usize,
    /// The Sound effect this object has
    sfx: S,
    /// The world XY coords of this object
    x: f32,
    y: f32,
    /// Get the angle of this object in radians
    angle: f32,
    /// Channel allocated to it (internal)
    channel: i32,
    /// Priority of sound
    priority: i32,
}

struct SfxInfo {
    /// Up to 6-character name. In the Lump the names are typically prefixed by `DS` or `DP`, so
    /// the full Lump name is 8-char, while the name here has the prefix striped off.
    name: String,
    /// Priority of sound
    priority: i32,

    // Not really used
    pitch: i32,
    volume: i32,

    /// Pre-processed SDL2 Chunk data
    data: Option<Chunk>,
    /// this is checked every second to see if sound can be thrown out (if 0,
    /// then decrement, if -1, then throw out, if > 0, then it is in use)
    usefulness: i32,
}

impl SfxInfo {
    pub(crate) fn new(name: String, priority: i32, data: Option<Chunk>) -> Self {
        Self {
            name,
            priority,
            pitch: -1,
            volume: -1,
            data,
            usefulness: 0,
        }
    }
}

/// `to_fmt` is almost always going to be `AudioFormat::S16LSB`, `to_rate` typically `44_100`.
pub(crate) fn lump_sfx_to_chunk(
    raw_lump: Vec<u8>,
    to_fmt: AudioFormat,
    to_rate: i32,
) -> Result<Chunk, String> {
    let rate = i16::from_le_bytes([raw_lump[2], raw_lump[3]]) as i32;
    let len = i32::from_le_bytes([raw_lump[4], raw_lump[5], raw_lump[6], raw_lump[7]]);
    let converter = AudioCVT::new(AudioFormat::U8, 1, rate, to_fmt, 2, to_rate)?;
    let fixed = converter.convert(raw_lump[7..len as usize].to_vec());

    sdl2::mixer::Chunk::from_raw_buffer(fixed.into_boxed_slice())
}

pub struct Snd {
    _audio: AudioSubsystem,
    _mixer: Sdl2MixerContext,
    rx: SndServerRx,
    tx: SndServerTx,
    kill: Arc<AtomicBool>,
    chunks: Vec<SfxInfo>,
    listener: SoundObject<SfxEnum>,
    sources: [SoundObject<SfxEnum>; MIXER_CHANNELS as usize],
}

unsafe impl Send for Snd {}

impl Snd {
    pub fn new(audio: AudioSubsystem, wad: &WadData) -> Result<Self, Box<dyn Error>> {
        // let mut timer = sdl.timer()?;
        let frequency = 44_100;
        let format = AUDIO_S16LSB; // signed 16 bit samples, in little-endian byte order
        let channels = DEFAULT_CHANNELS; // Stereo
        let chunk_size = 1_024;

        sdl2::mixer::open_audio(frequency, format, channels, chunk_size)?;
        let _mixer = sdl2::mixer::init(InitFlag::MOD | InitFlag::OGG)?;
        // Mixer channels are not play/stereo channels
        sdl2::mixer::allocate_channels(MIXER_CHANNELS);

        info!("Using sound driver: {}", audio.current_audio_driver());

        let chunks = SFX_INFO_BASE
            .iter()
            .map(|s| {
                let name = format!("DS{}", s.name.to_ascii_uppercase());
                if let Some(lump) = wad.get_lump(&name) {
                    let chunk = lump_sfx_to_chunk(lump.data.clone(), AudioFormat::S16LSB, 44_100)
                        .expect("{name} failed to parse");
                    SfxInfo::new(s.name.to_string(), s.priority, Some(chunk))
                } else {
                    warn!("{name} is missing");
                    SfxInfo::new(s.name.to_string(), s.priority, None)
                }
            })
            .collect();

        let (tx, rx) = channel();
        Ok(Self {
            _audio: audio,
            _mixer,
            rx,
            tx,
            chunks,
            kill: Arc::new(AtomicBool::new(false)),
            listener: SoundObject::default(),
            sources: [SoundObject::default(); MIXER_CHANNELS as usize],
        })
    }
}

impl SoundServer<SfxEnum, i32, sdl2::Error> for Snd {
    fn init(&mut self) -> InitResult<SfxEnum, i32, sdl2::Error> {
        Ok((self.tx.clone(), self.kill.clone()))
    }

    fn start_sound(&mut self, uid: usize, sfx: SfxEnum, x: f32, y: f32, angle: f32) {
        // TODO: temporary testing stuff here
        let dx = self.listener.x - x;
        let dy = self.listener.y - y;
        let mut dist = (dx.powf(2.0) + dy.powf(2.0)).sqrt().abs();
        if dist >= MAX_DIST {
            // Not audible
            return;
        }
        // Scale for SDL2
        dist = dist * 255.0 / MAX_DIST;
        let angle = point_to_angle_2(self.listener.x, self.listener.y, x, y);

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
            for c in 0..MIXER_CHANNELS {
                if !sdl2::mixer::Channel(c).is_playing() {
                    sdl2::mixer::Channel(c)
                        .set_position(angle as i16, dist as u8)
                        .unwrap();
                    sdl2::mixer::Channel(c).play(sfx, 0).unwrap();
                    origin.channel = c;
                    self.sources[c as usize] = origin;
                    return;
                }
            }

            // No free channel, need to evict a lower priority sound
            for c in 0..MIXER_CHANNELS {
                if self.sources[c as usize].priority <= origin.priority {
                    sdl2::mixer::Channel(c).expire(0);
                    sdl2::mixer::Channel(c)
                        .set_position(angle as i16, dist as u8)
                        .unwrap();
                    sdl2::mixer::Channel(c).play(sfx, 0).unwrap();
                    origin.channel = c;
                    self.sources[c as usize] = origin;
                }
            }
        }
    }

    fn update_listener(&mut self, x: f32, y: f32, angle: f32) {
        self.listener.x = x;
        self.listener.y = y;
        self.listener.angle = angle;

        for s in self.sources.iter_mut() {
            if s.channel != -1 && sdl2::mixer::Channel(s.channel).is_playing() {
                let dx = self.listener.x - s.x;
                let dy = self.listener.y - s.y;
                let dist = (dx.powf(2.0) + dy.powf(2.0)).sqrt().abs() * 255.0 / MAX_DIST;

                // Is it too far away now?
                if dist >= MAX_DIST {
                    sdl2::mixer::Channel(s.channel).expire(0);
                }

                let angle = point_to_angle_2(self.listener.x, self.listener.y, s.x, s.y);

                sdl2::mixer::Channel(s.channel)
                    .set_position(angle as i16, dist as u8)
                    .unwrap();
            } else {
                s.channel = -1;
            }
        }
    }

    fn stop_sound(&mut self, uid: usize) {
        for s in self.sources.iter() {
            if s.uid == uid {
                if sdl2::mixer::Channel(s.channel).is_playing() {
                    sdl2::mixer::Channel(s.channel).expire(0);
                }
            }
        }
    }

    fn set_sfx_volume(&mut self, volume: i32) {
        sdl2::mixer::Channel::all().set_volume(volume);
    }

    fn get_sfx_volume(&mut self) -> i32 {
        sdl2::mixer::Channel::all().get_volume()
    }

    fn start_music(&mut self, music: i32, looping: bool) {
        dbg!(music);
    }

    fn pause_music(&mut self) {}

    fn resume_music(&mut self) {}

    fn change_music(&mut self, _music: i32, _looping: bool) {}

    fn stop_music(&mut self) {}

    fn set_mus_volume(&mut self, volume: i32) {
        sdl2::mixer::Music::set_volume(volume)
    }

    fn get_mus_volume(&mut self) -> i32 {
        sdl2::mixer::Music::get_volume()
    }

    fn update_self(&mut self) {}

    fn get_rx(&mut self) -> &mut SndServerRx {
        &mut self.rx
    }

    fn get_shutdown(&self) -> &AtomicBool {
        self.kill.as_ref()
    }

    fn shutdown_sound(&mut self) {
        println!("Shutdown sound server")
    }
}

impl SoundServerTic<SfxEnum, i32, sdl2::Error> for Snd {}
