use std::{
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender},
        Arc,
    },
    time::Duration,
};

pub type InitResult<S, E> = Result<(Sender<SoundAction<S>>, Arc<AtomicBool>), E>;

pub enum SoundAction<S: Debug> {
    Start(S),
    Update(S),
    Stop(S),
}

/// A sound server implementing `SoundServer` must also implement `SoundServerTic`
/// typically by a one-liner: `impl SoundServerTic<SndFx> for Snd {}`
pub trait SoundServer<S, E>
where
    S: Debug,
    E: std::error::Error,
{
    /// Start up all sound stuff and grab the `Sender` channel for cloning, and an
    /// `AtomicBool` to stop sound and deintialise devices etc in preparation for
    /// game exit.
    fn init_sound(&mut self) -> InitResult<S, E>;

    /// Playback a sound
    fn start_sound(&mut self, sound: S);

    /// Update a sounds parameters
    fn update_sound(&mut self, sound: S);

    /// Stop this sound playback
    fn stop_sound(&mut self, sound: S);

    /// Helper function used by the `SoundServerTic` trait
    fn get_rx(&mut self) -> &mut Receiver<SoundAction<S>>;

    /// Atomic for shutting down the `SoundServer`
    fn get_shutdown(&self) -> &AtomicBool;

    /// Stop all sound and release the sound device
    fn shutdown_sound(&mut self);
}

/// Run the `SoundServer`
pub trait SoundServerTic<S, E>
where
    Self: SoundServer<S, E>,
    S: Debug,
    E: std::error::Error,
{
    /// Will be called every period on a thread containing `SoundServer`
    fn tic(&mut self) {
        if let Ok(sound) = self.get_rx().recv_timeout(Duration::from_micros(500)) {
            match sound {
                SoundAction::Start(s) => self.start_sound(s),
                SoundAction::Update(s) => self.update_sound(s),
                SoundAction::Stop(s) => self.stop_sound(s),
            }
        }
        if self.get_shutdown().load(Ordering::SeqCst) {
            self.get_shutdown().store(false, Ordering::Relaxed);
            self.shutdown_sound();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        fmt::Display,
        sync::{
            atomic::AtomicBool,
            mpsc::{channel, Receiver, Sender},
            Arc,
        },
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

    #[derive(Debug)]
    enum SndFx {
        One,
    }

    struct Snd {
        rx: Receiver<SoundAction<SndFx>>,
        tx: Sender<SoundAction<SndFx>>,
        kill: Arc<AtomicBool>,
    }

    impl Snd {
        fn new() -> Self {
            let (tx, rx) = channel();
            Self {
                rx,
                tx,
                kill: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl SoundServer<SndFx, FxError> for Snd {
        fn init_sound(&mut self) -> InitResult<SndFx, FxError> {
            Ok((self.tx.clone(), self.kill.clone()))
        }

        fn start_sound(&mut self, sound: SndFx) {
            dbg!(sound);
        }

        fn update_sound(&mut self, sound: SndFx) {
            dbg!(sound);
        }

        fn stop_sound(&mut self, sound: SndFx) {
            dbg!(sound);
        }

        fn get_rx(&mut self) -> &mut Receiver<SoundAction<SndFx>> {
            &mut self.rx
        }

        fn get_shutdown(&self) -> &AtomicBool {
            self.kill.as_ref()
        }

        fn shutdown_sound(&mut self) {
            todo!()
        }
    }

    impl SoundServerTic<SndFx, FxError> for Snd {}

    #[test]
    fn run_tic() {
        let mut snd = Snd::new();
        let (tx, _kill) = snd.init_sound().unwrap();

        tx.send(SoundAction::Start(SndFx::One)).unwrap();
        tx.send(SoundAction::Update(SndFx::One)).unwrap();
        tx.send(SoundAction::Stop(SndFx::One)).unwrap();
        assert_eq!(snd.rx.try_iter().count(), 3);

        tx.send(SoundAction::Start(SndFx::One)).unwrap();
        tx.send(SoundAction::Update(SndFx::One)).unwrap();
        tx.send(SoundAction::Stop(SndFx::One)).unwrap();
        for _ in 0..3 {
            snd.tic();
        }

        assert_eq!(snd.rx.try_iter().count(), 0);
    }
}
