use std::{
    error::Error,
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
use sound_traits::{
    InitResult, SfxEnum, SoundAction, SoundObjPosition, SoundServer, SoundServerTic,
};
use wad::WadData;

use crate::info::SFX_INFO_BASE;

mod info;

#[cfg(test)]
mod test_sdl2;

pub type SndServerRx = Receiver<SoundAction<SfxEnum, i32>>;
pub type SndServerTx = Sender<SoundAction<SfxEnum, i32>>;

pub fn point_to_angle_2(point1: (f32, f32), point2: (f32, f32)) -> f32 {
    let x = point1.0 - point2.0;
    let y = point1.1 - point2.1;
    y.atan2(x)
}

pub(crate) struct SfxInfo {
    /// Up to 6-character name. In the Lump the names are typically prefixed by `DS` or `DP`, so
    /// the full Lump name is 8-char, while the name here has the prefix striped off.
    name: String,
    /// Sfx singularity (only one at a time)
    singularity: bool,
    /// Priority of sound
    priority: i32,

    pitch: i32,
    volume: i32,

    /// Pre-processed SDL2 Chunk data
    data: Option<Chunk>,
    /// this is checked every second to see if sound can be thrown out (if 0,
    /// then decrement, if -1, then throw out, if > 0, then it is in use)
    usefulness: i32,
}

impl SfxInfo {
    pub(crate) fn new(name: String, singularity: bool, priority: i32, data: Option<Chunk>) -> Self {
        Self {
            name,
            singularity,
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
    audio: AudioSubsystem,
    _mixer: Sdl2MixerContext,
    rx: SndServerRx,
    tx: SndServerTx,
    kill: Arc<AtomicBool>,
    chunks: Vec<SfxInfo>,
    listener: SoundObjPosition,
    sources: Vec<SoundObjPosition>,
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
        let _mixer = sdl2::mixer::init(InitFlag::MP3 | InitFlag::MOD | InitFlag::OGG)?;
        // Mixer channels are not play/stereo channels
        sdl2::mixer::allocate_channels(16);

        info!("Using sound driver: {}", audio.current_audio_driver());

        let chunks = SFX_INFO_BASE
            .iter()
            .map(|s| {
                let name = format!("DS{}", s.name.to_ascii_uppercase());
                if let Some(lump) = wad.get_lump(&name) {
                    let chunk = lump_sfx_to_chunk(lump.data.clone(), AudioFormat::S16LSB, 44_100)
                        .expect("{name} failed to parse");
                    SfxInfo::new(s.name.to_string(), s.singularity, s.priority, Some(chunk))
                } else {
                    warn!("{name} is missing");
                    SfxInfo::new(s.name.to_string(), s.singularity, s.priority, None)
                }
            })
            .collect();

        let (tx, rx) = channel();
        Ok(Self {
            audio,
            _mixer,
            rx,
            tx,
            chunks,
            kill: Arc::new(AtomicBool::new(false)),
            listener: SoundObjPosition::default(),
            sources: Vec::new(),
        })
    }
}

impl SoundServer<SfxEnum, i32, sdl2::Error> for Snd {
    fn init(&mut self) -> InitResult<SfxEnum, i32, sdl2::Error> {
        Ok((self.tx.clone(), self.kill.clone()))
    }

    fn start_sound(&mut self, mut origin: SoundObjPosition, sound: SfxEnum) {
        // TODO: temporary testing stuff here
        let dx = self.listener.x() - origin.x();
        let dy = self.listener.y() - origin.y();
        let dist = (dx.powf(2.0) + dy.powf(2.0)).sqrt().abs() * 35.0 / 255.0;
        let angle = point_to_angle_2(self.listener.pos(), origin.pos());

        self.stop_sound(origin.uid());

        if let Some(sfx) = self.chunks[sound as usize].data.as_ref() {
            for obj in self.sources.iter() {
                if obj.uid() == origin.uid() {
                    if self.chunks[sound as usize].singularity {
                        return; // Don't play, it already is.
                    }
                    break;
                }
            }
            for c in 0..16 {
                if !sdl2::mixer::Channel(c).is_playing()  {
                    sdl2::mixer::Channel(c)
                        .set_position(angle as i16, dist as u8)
                        .unwrap();
                    sdl2::mixer::Channel(c).play(sfx, 0).unwrap();
                    origin.channel = c;
                    self.sources.push(origin);
                    break;
                }
            }
        }
    }

    fn update_sound(&mut self, listener: SoundObjPosition) {
        // TODO: Not efficient at all
        self.sources = self
            .sources
            .iter()
            .filter_map(|s| {
                if !sdl2::mixer::Channel(s.channel).is_playing() {
                    return None;
                }

                let dx = listener.x() - s.x();
                let dy = listener.y() - s.y();
                let dist = (dx.powf(2.0) + dy.powf(2.0)).sqrt().abs() * 35.0 / 255.0;
                let angle = point_to_angle_2(self.listener.pos(), s.pos());

                sdl2::mixer::Channel(s.channel)
                    .set_position(angle as i16, dist as u8)
                    .unwrap();
                Some(*s)
            })
            .collect();

        self.listener = listener;
    }

    fn stop_sound(&mut self, uid: usize) {
        self.sources = self
            .sources
            .iter()
            .filter_map(|s| {
                if s.uid() == uid {
                    sdl2::mixer::Channel(s.channel).expire(0);
                    return None;
                }
                Some(*s)
            })
            .collect();
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

    fn set_mus_volume(&mut self, volume: i32) {}

    fn get_mus_volume(&mut self) -> i32 {
        128
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
