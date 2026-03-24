use log::warn;
use opl2_emulator::OplPlayerState;
use sdl2::AudioSubsystem;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use std::sync::{Arc, Mutex};
use wad::WadData;

struct OplAudioCallback {
    state: Arc<Mutex<OplPlayerState>>,
    volume: Arc<Mutex<i32>>,
}

impl AudioCallback for OplAudioCallback {
    type Channel = i16;

    fn callback(&mut self, out: &mut [i16]) {
        if let Ok(mut state) = self.state.lock() {
            if let Ok(volume) = self.volume.lock() {
                state.volume = *volume;
            }
            state.generate_samples(out);
        } else {
            out.fill(0);
        }
    }
}

pub struct OplPlayer {
    _device: AudioDevice<OplAudioCallback>,
    state: Arc<Mutex<OplPlayerState>>,
    volume: Arc<Mutex<i32>>,
}

impl OplPlayer {
    pub fn new(audio: &AudioSubsystem, wad: &WadData) -> Result<Self, String> {
        let desired_spec = AudioSpecDesired {
            freq: Some(44100),
            channels: Some(1),
            samples: Some(512),
        };

        let state = Arc::new(Mutex::new(OplPlayerState::new(44100, wad)));
        let callback_state = Arc::clone(&state);
        let volume = Arc::new(Mutex::new(128));
        let callback_volume = Arc::clone(&volume);

        let device = audio.open_playback(None, &desired_spec, |spec| {
            if spec.freq != 44100 {
                warn!(
                    "[OPL] requested 44100 Hz, got {} Hz — reinitializing",
                    spec.freq
                );
                if let Ok(mut s) = callback_state.lock() {
                    s.reinit_rate(spec.freq as u32);
                }
            }
            OplAudioCallback {
                state: Arc::clone(&callback_state),
                volume: callback_volume,
            }
        })?;

        Ok(Self {
            _device: device,
            state,
            volume,
        })
    }

    pub fn load_music(&mut self, midi_data: &[u8]) -> Result<(), String> {
        if let Ok(mut state) = self.state.lock() {
            state.load_music(midi_data)
        } else {
            Err("Failed to lock OPL player state".to_string())
        }
    }

    pub fn play(&mut self, looping: bool) -> Result<(), String> {
        if let Ok(mut state) = self.state.lock() {
            state.start_playback(looping);
            self._device.resume();
            Ok(())
        } else {
            Err("Failed to lock OPL player state".to_string())
        }
    }

    pub fn stop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            state.stop_playback();
        }
        self._device.pause();
    }

    pub fn pause(&mut self) {
        self._device.pause();
    }

    pub fn resume(&mut self) {
        self._device.resume();
    }

    pub fn set_volume(&mut self, volume: i32) {
        if let Ok(mut vol) = self.volume.lock() {
            *vol = volume.clamp(0, 128);
        }

        if let Ok(mut state) = self.state.lock() {
            state.refresh_all_volumes();
        }
    }

    pub fn get_volume(&self) -> i32 {
        self.volume.lock().map(|v| *v).unwrap_or(64)
    }

    pub fn is_playing(&self) -> Result<bool, String> {
        if let Ok(state) = self.state.lock() {
            Ok(state.is_playing())
        } else {
            Err("Failed to lock OPL player state".to_string())
        }
    }
}
