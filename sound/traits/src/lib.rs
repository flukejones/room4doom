use std::fmt::Debug;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

mod sounds;
pub use sounds::*;
mod music;
pub use music::*;
pub mod info;
pub use info::{SFX_INFO_BASE, SfxInfoBase};
pub mod mus2midi;
pub use mus2midi::read_mus_to_midi;
pub mod spatial;
pub use spatial::*;

pub type InitResult<S, E> = Result<Sender<SoundAction<S>>, E>;

pub type SndServerTx = Sender<SoundAction<SfxName>>;
pub type SndServerRx = Receiver<SoundAction<SfxName>>;

pub enum SoundAction<S: Debug + Copy> {
    StartSfx {
        uid: usize,
        sfx: S,
        x: f32,
        y: f32,
    },
    UpdateListener {
        uid: usize,
        x: f32,
        y: f32,
        angle: f32,
    },
    StopSfx {
        uid: usize,
    },
    StopSfxAll,
    SfxVolume(i32),
    MusicVolume(i32),
    StartMusic(Vec<u8>, bool),
    PauseMusic,
    ResumeMusic,
    ChangeMusic(Vec<u8>, bool),
    StopMusic,
    SetMusicType(i32),
    Shutdown,
}

pub trait SoundServer<S, E>
where
    S: Debug + Copy,
    E: std::error::Error,
{
    fn init(&mut self) -> InitResult<S, E>;
    fn start_sound(&mut self, uid: usize, sfx: S, x: f32, y: f32);
    fn update_listener(&mut self, uid: usize, x: f32, y: f32, angle: f32);
    fn stop_sound(&mut self, uid: usize);
    fn stop_sound_all(&mut self);
    fn set_sfx_volume(&mut self, volume: i32);
    fn get_sfx_volume(&mut self) -> i32;
    fn start_music(&mut self, data: Vec<u8>, looping: bool);
    fn pause_music(&mut self);
    fn resume_music(&mut self);
    fn change_music(&mut self, data: Vec<u8>, looping: bool);
    fn stop_music(&mut self);
    fn set_music_type(&mut self, music_type: i32);
    fn set_mus_volume(&mut self, volume: i32);
    fn get_mus_volume(&mut self) -> i32;
    fn update_self(&mut self);
    fn get_rx(&mut self) -> &mut Receiver<SoundAction<S>>;
    fn shutdown_sound(&mut self);
}

pub trait SoundServerTic<S, E>
where
    Self: SoundServer<S, E>,
    S: Debug + Copy,
    E: std::error::Error,
{
    fn tic(&mut self) -> bool {
        if let Ok(sound) = self.get_rx().recv_timeout(Duration::from_micros(500)) {
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
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fmt::Display;
    use std::sync::mpsc::{Receiver, Sender, channel};

    use crate::{InitResult, SoundAction, SoundServer, SoundServerTic};

    #[derive(Debug)]
    enum FxError {}

    impl Error for FxError {}

    impl Display for FxError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&format!("{:?}", self))
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum SndFx {
        One,
    }

    struct Snd {
        rx: Receiver<SoundAction<SndFx>>,
        tx: Sender<SoundAction<SndFx>>,
    }

    impl Snd {
        fn new() -> Self {
            let (tx, rx) = channel();
            Self {
                rx,
                tx,
            }
        }
    }

    impl SoundServer<SndFx, FxError> for Snd {
        fn init(&mut self) -> InitResult<SndFx, FxError> {
            Ok(self.tx.clone())
        }

        fn start_sound(&mut self, uid: usize, sfx: SndFx, x: f32, y: f32) {
            dbg!(uid, sfx, x, y);
        }

        fn update_listener(&mut self, uid: usize, x: f32, y: f32, angle: f32) {
            dbg!(uid, x, y, angle);
        }

        fn stop_sound(&mut self, uid: usize) {
            dbg!(uid);
        }

        fn stop_sound_all(&mut self) {}
        fn set_sfx_volume(&mut self, _volume: i32) {}
        fn get_sfx_volume(&mut self) -> i32 {
            6
        }
        fn start_music(&mut self, _data: Vec<u8>, _looping: bool) {}
        fn pause_music(&mut self) {}
        fn resume_music(&mut self) {}
        fn change_music(&mut self, _data: Vec<u8>, _looping: bool) {}
        fn stop_music(&mut self) {}
        fn set_music_type(&mut self, _: i32) {}
        fn set_mus_volume(&mut self, _volume: i32) {}
        fn get_mus_volume(&mut self) -> i32 {
            7
        }
        fn update_self(&mut self) {}

        fn get_rx(&mut self) -> &mut Receiver<SoundAction<SndFx>> {
            &mut self.rx
        }

        fn shutdown_sound(&mut self) {
            todo!()
        }
    }

    impl SoundServerTic<SndFx, FxError> for Snd {}

    #[test]
    fn sound_server_test() {
        let mut snd = Snd::new();
        let tx = snd.init().unwrap();

        tx.send(SoundAction::StartSfx {
            uid: 1,
            sfx: SndFx::One,
            x: 320.0,
            y: 200.0,
        })
        .unwrap();

        assert!(snd.tic());
    }
}
