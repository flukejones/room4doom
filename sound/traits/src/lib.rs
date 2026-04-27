use std::fmt::Debug;
use std::sync::mpsc::{Receiver, Sender};

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

/// Result returned by a sound backend's `init()`. The `Ok` arm carries
/// the sender side of the action channel; the caller broadcasts
/// `SoundAction` messages through it from gameplay/gamestate threads.
pub type InitResult<S, E> = Result<Sender<SoundAction<S>>, E>;

pub type SndServerTx = Sender<SoundAction<SfxName>>;
pub type SndServerRx = Receiver<SoundAction<SfxName>>;

/// Cross-thread message protocol between gameplay/gamestate (producers)
/// and the sound backend's tic loop (consumer).
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
