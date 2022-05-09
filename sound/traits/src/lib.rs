use std::{
    fmt::Debug,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};

mod sounds;
pub use sounds::*;
mod music;
pub use music::*;

/// `S` is SFX enum, `M` is Music enum, `E` is Errors
pub type InitResult<S, M, E> = Result<Sender<SoundAction<S, M>>, E>;

pub enum SoundAction<S: Debug + Copy, M: Debug> {
    StartSfx {
        /// Objects unique ID or hash. This should be used to track which
        /// object owns which sounds so it can be stopped e.g, death, shoot..
        uid: usize,
        /// The Sound effect this object has
        sfx: S,
        /// The world XY coords of this object
        x: f32,
        y: f32,
    },
    /// Where in the world the listener is
    UpdateListener {
        /// UID of the listener. This should be used to help position or stop
        /// sounds relative to the player
        uid: usize,
        /// The world XY coords of this object
        x: f32,
        y: f32,
        /// Get the angle of this object in radians
        angle: f32,
    },
    StopSfx {
        uid: usize,
    },
    StopSfxAll,
    SfxVolume(i32),
    MusicVolume(i32),

    /// Music ID and looping/not
    StartMusic(M, bool),
    PauseMusic,
    ResumeMusic,
    ChangeMusic(M, bool),
    StopMusic,
    Shutdown,
}

/// A sound server implementing `SoundServer` must also implement `SoundServerTic`
/// typically by a one-liner: `impl SoundServerTic<SndFx> for Snd {}`
pub trait SoundServer<S, M, E>
where
    S: Debug + Copy,
    M: Debug,
    E: std::error::Error,
{
    /// Start up all sound stuff and grab the `Sender` channel for cloning, and an
    /// `AtomicBool` to stop sound and deinitialise devices etc in preparation for
    /// game-exe exit.
    fn init(&mut self) -> InitResult<S, M, E>;

    /// Playback a sound
    fn start_sound(&mut self, uid: usize, sfx: S, x: f32, y: f32);

    /// Update a sounds parameters
    fn update_listener(&mut self, uid: usize, x: f32, y: f32, angle: f32);

    /// Stop this sound playback
    fn stop_sound(&mut self, uid: usize);

    fn stop_sound_all(&mut self);

    fn set_sfx_volume(&mut self, volume: i32);

    fn get_sfx_volume(&mut self) -> i32;

    fn start_music(&mut self, music: M, looping: bool);

    fn pause_music(&mut self);

    fn resume_music(&mut self);

    fn change_music(&mut self, music: M, looping: bool);

    fn stop_music(&mut self);

    fn set_mus_volume(&mut self, volume: i32);

    fn get_mus_volume(&mut self) -> i32;

    /// Start, stop, change, remove sounds. Anythign that a sound server needs
    /// to do each tic
    fn update_self(&mut self);

    /// Helper function used by the `SoundServerTic` trait
    fn get_rx(&mut self) -> &mut Receiver<SoundAction<S, M>>;

    /// Stop all sound and release the sound device
    fn shutdown_sound(&mut self);
}

/// Run the `SoundServer`
pub trait SoundServerTic<S, M, E>
where
    Self: SoundServer<S, M, E>,
    S: Debug + Copy,
    M: Debug,
    E: std::error::Error,
{
    /// Will be called every period on a thread containing `SoundServer`, returns
    /// `true` if the thread should continue running, else `false` if it should exit.
    fn tic(&mut self) -> bool {
        if let Ok(sound) = self.get_rx().recv_timeout(Duration::from_micros(500)) {
            match sound {
                SoundAction::StartSfx { uid, sfx, x, y } => self.start_sound(uid, sfx, x, y),
                SoundAction::UpdateListener { uid, x, y, angle } => {
                    self.update_listener(uid, x, y, angle)
                }
                SoundAction::StopSfx { uid } => self.stop_sound(uid),
                SoundAction::StopSfxAll => self.stop_sound_all(),
                SoundAction::StartMusic(music, looping) => self.start_music(music, looping),
                SoundAction::PauseMusic => self.pause_music(),
                SoundAction::ResumeMusic => self.resume_music(),
                SoundAction::ChangeMusic(music, looping) => self.change_music(music, looping),
                SoundAction::StopMusic => self.stop_music(),
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
    use std::{
        error::Error,
        f32::consts::PI,
        fmt::Display,
        sync::mpsc::{channel, Receiver, Sender},
    };

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

    #[derive(Debug)]
    enum Music {}

    struct Snd {
        rx: Receiver<SoundAction<SndFx, Music>>,
        tx: Sender<SoundAction<SndFx, Music>>,
    }

    impl Snd {
        fn new() -> Self {
            let (tx, rx) = channel();
            Self { rx, tx }
        }
    }

    impl SoundServer<SndFx, Music, FxError> for Snd {
        fn init(&mut self) -> InitResult<SndFx, Music, FxError> {
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

        fn start_music(&mut self, music: Music, _looping: bool) {
            dbg!(music);
        }

        fn pause_music(&mut self) {}

        fn resume_music(&mut self) {}

        fn change_music(&mut self, _music: Music, _looping: bool) {}

        fn stop_music(&mut self) {}

        fn set_sfx_volume(&mut self, _volume: i32) {}

        fn get_sfx_volume(&mut self) -> i32 {
            6
        }

        fn set_mus_volume(&mut self, _volume: i32) {}

        fn get_mus_volume(&mut self) -> i32 {
            7
        }

        fn update_self(&mut self) {}

        fn get_rx(&mut self) -> &mut Receiver<SoundAction<SndFx, Music>> {
            &mut self.rx
        }

        fn shutdown_sound(&mut self) {
            todo!()
        }
    }

    impl SoundServerTic<SndFx, Music, FxError> for Snd {}

    #[test]
    fn run_tic() {
        let mut snd = Snd::new();
        let tx = snd.init().unwrap();

        tx.send(SoundAction::StartSfx {
            uid: 123,
            sfx: SndFx::One,
            x: 0.3,
            y: 0.3,
        })
        .unwrap();
        tx.send(SoundAction::UpdateListener {
            uid: 42,
            x: 0.3,
            y: 0.3,
            angle: PI / 2.0,
        })
        .unwrap();
        tx.send(SoundAction::StopSfx { uid: 123 }).unwrap();
        assert_eq!(snd.rx.try_iter().count(), 3);

        tx.send(SoundAction::StartSfx {
            uid: 123,
            sfx: SndFx::One,
            x: 0.3,
            y: 0.3,
        })
        .unwrap();
        tx.send(SoundAction::UpdateListener {
            uid: 42,
            x: 0.3,
            y: 0.3,
            angle: PI / 2.0,
        })
        .unwrap();
        tx.send(SoundAction::StopSfx { uid: 123 }).unwrap();
        for _ in 0..3 {
            snd.tic();
        }

        assert_eq!(snd.rx.try_iter().count(), 0);
    }
}
