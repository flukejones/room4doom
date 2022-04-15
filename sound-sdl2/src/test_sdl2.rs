use std::{
    error::Error,
    f32::consts::PI,
    fs::OpenOptions,
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
};

use crate::lump_sfx_to_chunk;
use gameplay::SfxEnum;
use sdl2::{
    audio::AudioFormat,
    mixer::{Chunk, InitFlag, Sdl2MixerContext, AUDIO_S16LSB, DEFAULT_CHANNELS},
    AudioSubsystem,
};
use sound_traits::{InitResult, ObjectPositioning, SoundAction, SoundServer, SoundServerTic};

#[derive(Debug, Clone, Copy)]
enum Mus {
    One,
    Two,
}

struct Snd {
    audio: AudioSubsystem,
    _mixer: Sdl2MixerContext,
    rx: Receiver<SoundAction<SfxEnum, Mus>>,
    tx: Sender<SoundAction<SfxEnum, Mus>>,
    kill: Arc<AtomicBool>,
    test_chunk1: Chunk,
    test_chunk2: Chunk,
    test_pistol: Chunk,
}

unsafe impl Send for Snd {}

impl Snd {
    fn new(audio: AudioSubsystem) -> Result<Self, Box<dyn Error>> {
        // let mut timer = sdl.timer()?;
        let frequency = 44_100;
        let format = AUDIO_S16LSB; // signed 16 bit samples, in little-endian byte order
        let channels = DEFAULT_CHANNELS; // Stereo
        let chunk_size = 1_024;

        sdl2::mixer::open_audio(frequency, format, channels, chunk_size)?;
        let _mixer = sdl2::mixer::init(InitFlag::MP3 | InitFlag::MOD | InitFlag::OGG)?;
        sdl2::mixer::allocate_channels(16);

        dbg!(audio.current_audio_driver());

        // One second of 500Hz sine wave using equation A * sin(2 * PI * f * t)
        // (played at half the volume to save people's ears).
        let buffer = (0..44_100)
            .map(|i| {
                (0.07 * i16::MAX as f32 * (0.777 * PI * 500.0 * (i as f32 / 44_100_f32)).sin())
                    as i16
            })
            .collect();
        let test_chunk1 = sdl2::mixer::Chunk::from_raw_buffer(buffer)
            .map_err(|e| format!("Cannot get chunk from buffer: {:?}", e))?;

        let buffer = (0..44_100)
            .map(|i| {
                (0.1 * i16::MAX as f32 * (1.0 * PI * 500.0 * (i as f32 / 44_100_f32)).sin()) as i16
            })
            .collect();
        let test_chunk2 = sdl2::mixer::Chunk::from_raw_buffer(buffer)
            .map_err(|e| format!("Cannot get chunk from buffer: {:?}", e))?;

        let mut options = OpenOptions::new();
        let mut file = options.read(true).open("data/DSPISTOL.lmp")?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let test_pistol = lump_sfx_to_chunk(buf, AudioFormat::S16LSB, 44_100)?;

        let (tx, rx) = channel();
        Ok(Self {
            audio,
            _mixer,
            rx,
            tx,
            test_chunk1,
            test_chunk2,
            test_pistol,
            kill: Arc::new(AtomicBool::new(false)),
        })
    }
}

impl SoundServer<SfxEnum, Mus, sdl2::Error> for Snd {
    fn init_sound(&mut self) -> InitResult<SfxEnum, Mus, sdl2::Error> {
        Ok((self.tx.clone(), self.kill.clone()))
    }

    fn start_sound(
        &mut self,
        origin: ObjectPositioning,
        player: ObjectPositioning,
        sound: SfxEnum,
    ) {
        dbg!(sound);
        match sound {
            SfxEnum::bfg => {
                sdl2::mixer::Channel(0).play(&self.test_chunk1, 2).unwrap();
            }
            SfxEnum::barexp => {
                sdl2::mixer::Channel(1).play(&self.test_chunk2, 6).unwrap();
            }
            SfxEnum::pistol => {
                sdl2::mixer::Channel::all()
                    .play(&self.test_pistol, 0)
                    .unwrap();
            }
            _ => {}
        };
    }

    fn update_sound(&mut self, listener: ObjectPositioning) {
        dbg!(self.audio.current_audio_driver());
        dbg!(listener);
    }

    fn stop_sound(&mut self, uid: usize) {
        dbg!(uid);
        match uid {
            123 => sdl2::mixer::Channel(0).pause(),
            456 => sdl2::mixer::Channel(1).pause(),
            _ => {}
        };
    }

    fn set_sfx_volume(&mut self, volume: f32) {}

    fn get_sfx_volume(&mut self) -> f32 {
        6.66
    }

    fn start_music(&mut self, music: Mus, looping: bool) {
        dbg!(music);
    }

    fn pause_music(&mut self) {}

    fn resume_music(&mut self) {}

    fn change_music(&mut self, _music: Mus, _looping: bool) {}

    fn stop_music(&mut self) {}

    fn set_mus_volume(&mut self, volume: f32) {}

    fn get_mus_volume(&mut self) -> f32 {
        7.77
    }

    fn get_rx(&mut self) -> &mut Receiver<SoundAction<SfxEnum, Mus>> {
        &mut self.rx
    }

    fn get_shutdown(&self) -> &AtomicBool {
        self.kill.as_ref()
    }

    fn shutdown_sound(&mut self) {
        println!("Shutdown sound server")
    }
}

impl SoundServerTic<SfxEnum, Mus, sdl2::Error> for Snd {}

#[test]
fn run_tic() {
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap()).unwrap();
    let (tx, kill) = snd.init_sound().unwrap();

    let _thread = std::thread::spawn(move || loop {
        snd.tic();
        std::thread::sleep(std::time::Duration::from_millis(5));
    });

    tx.send(SoundAction::StartSfx {
        origin: ObjectPositioning::new(123, (0.3, 0.3), PI),
        player: ObjectPositioning::new(123, (1.0, 1.0), PI / 2.0),
        sfx: SfxEnum::bfg,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(333));

    tx.send(SoundAction::StartSfx {
        origin: ObjectPositioning::new(123, (0.3, 0.3), PI),
        player: ObjectPositioning::new(123, (1.0, 1.0), PI / 2.0),
        sfx: SfxEnum::barexp,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(333));

    tx.send(SoundAction::UpdateSfx {
        listener: ObjectPositioning::new(123, (1.0, 1.0), PI / 2.0),
    })
    .unwrap();

    tx.send(SoundAction::StopSfx { uid: 456 }).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));

    kill.store(true, Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[test]
fn play_pistol_snd() {
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap()).unwrap();
    let (tx, kill) = snd.init_sound().unwrap();

    let _thread = std::thread::spawn(move || loop {
        snd.tic();
        std::thread::sleep(std::time::Duration::from_millis(5));
    });

    tx.send(SoundAction::StartSfx {
        origin: ObjectPositioning::new(123, (0.3, 0.3), PI),
        player: ObjectPositioning::new(123, (1.0, 1.0), PI / 2.0),
        sfx: SfxEnum::pistol,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        origin: ObjectPositioning::new(123, (0.3, 0.3), PI),
        player: ObjectPositioning::new(123, (1.0, 1.0), PI / 2.0),
        sfx: SfxEnum::pistol,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    kill.store(true, Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[test]
fn play_music() {
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap()).unwrap();
    let (tx, kill) = snd.init_sound().unwrap();

    let _thread = std::thread::spawn(move || loop {
        snd.tic();
        std::thread::sleep(std::time::Duration::from_millis(5));
    });

    tx.send(SoundAction::StartMusic(Mus::One, false)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    kill.store(true, Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(500));
}
