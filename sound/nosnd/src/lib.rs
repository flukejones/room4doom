use std::error::Error;
use std::fmt::Display;
use std::sync::mpsc::channel;

use log::info;

use sound_common::{InitResult, SfxName, SoundServer, SoundServerTic};

use sound_common::{SndServerRx, SndServerTx};

pub struct Snd {
    rx: SndServerRx,
    tx: SndServerTx,
}

unsafe impl Send for Snd {}

impl Snd {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let (tx, rx) = channel();
        Ok(Self {
            rx,
            tx,
        })
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

impl SoundServer<SfxName, SndError> for Snd {
    fn init(&mut self) -> InitResult<SfxName, SndError> {
        Ok(self.tx.clone())
    }

    fn start_sound(&mut self, _: usize, _: SfxName, _: f32, _: f32) {}
    fn update_listener(&mut self, _: usize, _: f32, _: f32, _: f32) {}
    fn stop_sound(&mut self, _: usize) {}
    fn stop_sound_all(&mut self) {}
    fn set_sfx_volume(&mut self, _: i32) {}
    fn get_sfx_volume(&mut self) -> i32 {
        666
    }
    fn start_music(&mut self, _: Vec<u8>, _: bool) {}
    fn pause_music(&mut self) {}
    fn resume_music(&mut self) {}
    fn change_music(&mut self, _: Vec<u8>, _: bool) {}
    fn stop_music(&mut self) {}
    fn set_music_type(&mut self, _: i32) {}
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

impl SoundServerTic<SfxName, SndError> for Snd {}
