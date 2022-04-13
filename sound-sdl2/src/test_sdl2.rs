use std::{
    error::Error,
    f32::consts::PI,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
};

use sdl2::{
    mixer::{Chunk, InitFlag, Sdl2MixerContext, AUDIO_S16LSB, DEFAULT_CHANNELS},
    AudioSubsystem,
};
use sound_traits::{InitResult, SoundAction, SoundServer, SoundServerTic};

#[derive(Debug, Clone, Copy)]
enum SndFx {
    One,
    Two,
}

struct Snd {
    audio: AudioSubsystem,
    _mixer: Sdl2MixerContext,
    rx: Receiver<SoundAction<SndFx>>,
    tx: Sender<SoundAction<SndFx>>,
    kill: Arc<AtomicBool>,
    test_chunk1: Chunk,
    test_chunk2: Chunk,
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
        let _mixer =
            sdl2::mixer::init(InitFlag::MP3 | InitFlag::FLAC | InitFlag::MOD | InitFlag::OGG)?;
        sdl2::mixer::allocate_channels(16);

        dbg!(audio.current_audio_driver());

        // One second of 500Hz sine wave using equation A * sin(2 * PI * f * t)
        // (played at half the volume to save people's ears).
        let buffer = (0..44_100)
            .map(|i| {
                (0.07
                    * i16::max_value() as f32
                    * (0.777 * PI * 500.0 * (i as f32 / 44_100_f32)).sin()) as i16
            })
            .collect();
        let test_chunk1 = sdl2::mixer::Chunk::from_raw_buffer(buffer)
            .map_err(|e| format!("Cannot get chunk from buffer: {:?}", e))?;

        let buffer = (0..44_100)
            .map(|i| {
                (0.06
                    * i16::max_value() as f32
                    * (0.666 * PI * 500.0 * (i as f32 / 44_100_f32)).sin()) as i16
            })
            .collect();
        let test_chunk2 = sdl2::mixer::Chunk::from_raw_buffer(buffer)
            .map_err(|e| format!("Cannot get chunk from buffer: {:?}", e))?;

        let (tx, rx) = channel();
        Ok(Self {
            audio,
            _mixer,
            rx,
            tx,
            test_chunk1,
            test_chunk2,
            kill: Arc::new(AtomicBool::new(false)),
        })
    }
}

impl SoundServer<SndFx, sdl2::Error> for Snd {
    fn init_sound(&mut self) -> InitResult<SndFx, sdl2::Error> {
        Ok((self.tx.clone(), self.kill.clone()))
    }

    fn start_sound(&mut self, sound: SndFx) {
        dbg!(sound);
        match sound {
            SndFx::One => sdl2::mixer::Channel(0).play(&self.test_chunk1, 1).unwrap(),
            SndFx::Two => sdl2::mixer::Channel(1).play(&self.test_chunk2, 1).unwrap(),
        };
    }

    fn update_sound(&mut self, sound: SndFx) {
        dbg!(self.audio.current_audio_driver());
        dbg!(sound);
    }

    fn stop_sound(&mut self, sound: SndFx) {
        dbg!(sound);
        match sound {
            SndFx::One => sdl2::mixer::Channel(0).pause(),
            SndFx::Two => sdl2::mixer::Channel(1).pause(),
        };
    }

    fn get_rx(&mut self) -> &mut Receiver<SoundAction<SndFx>> {
        &mut self.rx
    }

    fn get_shutdown(&self) -> &AtomicBool {
        self.kill.as_ref()
    }

    fn shutdown_sound(&mut self) {
        println!("Shutdown sound server")
    }
}

impl SoundServerTic<SndFx, sdl2::Error> for Snd {}

#[test]
fn run_tic() {
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap()).unwrap();
    let (tx, kill) = snd.init_sound().unwrap();

    let _thread = std::thread::spawn(move || loop {
        snd.tic();
        std::thread::sleep(std::time::Duration::from_millis(5));
    });

    tx.send(SoundAction::Start(SndFx::One)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(333));

    tx.send(SoundAction::Start(SndFx::Two)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(333));

    tx.send(SoundAction::Stop(SndFx::Two)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));

    kill.store(true, Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(500));
}
