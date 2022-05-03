use std::{
    error::Error,
    f32::consts::PI,
    fmt::Debug,
    sync::mpsc::{channel, Receiver, Sender},
};

use glam::Vec2;
use log::{debug, info};
use sdl2::{
    audio::{AudioCVT, AudioFormat},
    mixer::{Chunk, InitFlag, Music, Sdl2MixerContext, AUDIO_S16LSB, DEFAULT_CHANNELS},
    AudioSubsystem,
};
use sound_traits::{InitResult, SfxNum, SoundAction, SoundServer, SoundServerTic, MUS_DATA};
use wad::WadData;

use crate::{info::SFX_INFO_BASE, mus2midi::read_mus_to_midi};

mod info;
pub mod mus2midi;
pub mod timidity;

#[cfg(test)]
mod test_sdl2;

const MAX_DIST: f32 = 1666.0;
const MIXER_CHANNELS: i32 = 32;
const MUS_ID: [u8; 4] = [b'M', b'U', b'S', 0x1a];
const MID_ID: [u8; 4] = [b'M', b'T', b'h', b'd'];

pub type SndServerRx = Receiver<SoundAction<SfxNum, usize>>;
pub type SndServerTx = Sender<SoundAction<SfxNum, usize>>;

pub fn point_to_angle_2(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let x = x1 - x2;
    let y = y1 - y2;
    y.atan2(x)
}

pub fn angle_between(listener_angle: f32, other_x: f32, other_y: f32) -> f32 {
    let (y, x) = listener_angle.sin_cos();
    let v1 = Vec2::new(x, y);
    let other = Vec2::new(other_x, other_y);
    v1.angle_between(other)
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
fn lump_sfx_to_chunk(
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

pub struct Snd<'a> {
    _audio: AudioSubsystem,
    _mixer: Sdl2MixerContext,
    rx: SndServerRx,
    tx: SndServerTx,
    chunks: Vec<SfxInfo>,
    music: Option<Music<'a>>,
    listener: SoundObject<SfxNum>,
    sources: [SoundObject<SfxNum>; MIXER_CHANNELS as usize],
    sfx_vol: i32,
    mus_vol: i32,
}

unsafe impl<'a> Send for Snd<'a> {}

impl<'a> Snd<'a> {
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

        let chunks: Vec<SfxInfo> = SFX_INFO_BASE
            .iter()
            .map(|s| {
                let name = format!("DS{}", s.name.to_ascii_uppercase());
                if let Some(lump) = wad.get_lump(&name) {
                    let chunk = lump_sfx_to_chunk(lump.data.clone(), AudioFormat::S16LSB, 44_100)
                        .expect("{name} failed to parse");
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

        let (tx, rx) = channel();
        Ok(Self {
            _audio: audio,
            _mixer,
            rx,
            tx,
            chunks,
            music: None,
            listener: SoundObject::default(),
            sources: [SoundObject::default(); MIXER_CHANNELS as usize],
            sfx_vol: 64,
            mus_vol: 64,
        })
    }

    fn listener_to_source_angle(&self, sx: f32, sy: f32) -> f32 {
        let (y, x) = point_to_angle_2(sx, sy, self.listener.x, self.listener.y).sin_cos();
        let mut angle = angle_between(self.listener.angle, x, y);
        if angle.is_sign_negative() {
            angle += PI * 2.0;
        }
        360.0 - angle.to_degrees()
    }

    fn dist_from_listener(&self, sx: f32, sy: f32) -> f32 {
        let dx = self.listener.x - sx;
        let dy = self.listener.y - sy;
        (dx.powf(2.0) + dy.powf(2.0)).sqrt().abs()
    }

    fn dist_scale_sdl2(dist: f32) -> f32 {
        dist * 255.0 / MAX_DIST
    }
}

impl<'a> SoundServer<SfxNum, usize, sdl2::Error> for Snd<'a> {
    fn init(&mut self) -> InitResult<SfxNum, usize, sdl2::Error> {
        Ok(self.tx.clone())
    }

    fn start_sound(&mut self, uid: usize, sfx: SfxNum, x: f32, y: f32) {
        let mut dist = self.dist_from_listener(x, y);
        if dist >= MAX_DIST {
            // Not audible
            return;
        }
        // Scale for SDL2
        dist = Self::dist_scale_sdl2(dist);
        let mut angle = 0.0;
        if uid != self.listener.uid {
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
            for c in 0..MIXER_CHANNELS {
                if !sdl2::mixer::Channel(c).is_playing() || sdl2::mixer::Channel(c).is_paused() {
                    // TODO: Set a volume for player sounds
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
            for c in 0..MIXER_CHANNELS {
                if self.sources[c as usize].priority >= origin.priority {
                    sdl2::mixer::Channel(c).halt();
                    // TODO: Set a volume for player sounds
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
        unsafe {
            let music = sdl2::mixer::Music::from_static_bytes(MUS_DATA[music].data()).unwrap();
            music.play(if looping { -1 } else { 0 }).unwrap();
            self.music = Some(music);
            sdl2::mixer::Music::set_volume(self.mus_vol);
        }
    }

    fn pause_music(&mut self) {
        sdl2::mixer::Music::pause();
    }

    fn resume_music(&mut self) {
        sdl2::mixer::Music::resume();
    }

    fn change_music(&mut self, music: usize, looping: bool) {
        sdl2::mixer::Music::halt();
        self.music.take();
        self.start_music(music, looping)
    }

    fn stop_music(&mut self) {
        sdl2::mixer::Music::halt();
    }

    fn set_mus_volume(&mut self, volume: i32) {
        sdl2::mixer::Music::set_volume(volume);
        self.mus_vol = volume;
    }

    fn get_mus_volume(&mut self) -> i32 {
        sdl2::mixer::Music::get_volume()
    }

    fn update_self(&mut self) {}

    fn get_rx(&mut self) -> &mut SndServerRx {
        &mut self.rx
    }

    fn shutdown_sound(&mut self) {
        info!("Shutdown sound server");
        self.stop_sound_all();
        self.stop_music();
    }
}

impl<'a> SoundServerTic<SfxNum, usize, sdl2::Error> for Snd<'a> {}

#[cfg(test)]
mod tests {
    use crate::mus2midi::read_mus_to_midi;
    use sdl2::mixer::{InitFlag, AUDIO_S16LSB, DEFAULT_CHANNELS};
    use sound_traits::MUS_DATA;
    use std::time::Duration;
    use wad::WadData;

    #[ignore = "CI doesn't have a sound device"]
    #[test]
    fn write_map_mus_data() {
        let wad = WadData::new("../doom1.wad".into());

        unsafe {
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
