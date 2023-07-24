use std::{
    error::Error,
    fmt::Display,
    sync::mpsc::{channel, Receiver, Sender},
};

use log::info;

use sound_traits::{InitResult, SfxName, SoundAction, SoundServer, SoundServerTic};
use wad::WadData;

pub type SndServerRx = Receiver<SoundAction<SfxName, usize>>;
pub type SndServerTx = Sender<SoundAction<SfxName, usize>>;

pub struct Snd {
    rx: SndServerRx,
    tx: SndServerTx,
}

unsafe impl Send for Snd {}

impl Snd {
    pub fn new(_: &WadData) -> Result<Self, Box<dyn Error>> {
        let (tx, rx) = channel();
        Ok(Self { rx, tx })
    }
}

#[derive(Debug)]
pub enum SndError {
    None,
}

impl Display for SndError {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

impl std::error::Error for SndError {}

impl SoundServer<SfxName, usize, SndError> for Snd {
    fn init(&mut self) -> InitResult<SfxName, usize, SndError> {
        Ok(self.tx.clone())
    }

    fn start_sound(&mut self, _: usize, _: SfxName, mut _x: f32, mut _y: f32) {}

    fn update_listener(&mut self, _: usize, _: f32, _: f32, _: f32) {}

    fn stop_sound(&mut self, _: usize) {}

    fn stop_sound_all(&mut self) {}

    fn set_sfx_volume(&mut self, _: i32) {}

    fn get_sfx_volume(&mut self) -> i32 {
        666
    }

    fn start_music(&mut self, _: usize, _: bool) {}

    fn pause_music(&mut self) {}

    fn resume_music(&mut self) {}

    fn change_music(&mut self, _: usize, _: bool) {}

    fn stop_music(&mut self) {}

    fn set_mus_volume(&mut self, _: i32) {}

    fn get_mus_volume(&mut self) -> i32 {
        666
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

impl SoundServerTic<SfxName, usize, SndError> for Snd {}
