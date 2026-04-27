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

pub type SndServerTx = Sender<SoundAction>;
pub type SndServerRx = Receiver<SoundAction>;

/// Music synthesizer selection. Mapped from the user config's MusicType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicType {
    OPL2,
    OPL3,
    GUS,
}

impl TryFrom<i32> for MusicType {
    type Error = i32;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::OPL2),
            1 => Ok(Self::OPL3),
            2 => Ok(Self::GUS),
            other => Err(other),
        }
    }
}

/// Cross-thread message protocol between gameplay/gamestate (producers)
/// and the sound backend's tic loop (consumer).
pub enum SoundAction {
    StartSfx {
        uid: usize,
        sfx: SfxName,
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
    SetMusicType(MusicType),
    Shutdown,
}
