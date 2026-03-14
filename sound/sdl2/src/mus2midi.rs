//! Re-export MUS-to-MIDI conversion from sound-common.
//!
//! SDL2-specific playback tests live here.

pub use sound_common::read_mus_to_midi;

#[cfg(test)]
mod tests {
    use std::env::set_var;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Duration;

    use sdl2::mixer::{AUDIO_S16LSB, DEFAULT_CHANNELS, InitFlag};
    use wad::WadData;

    use super::read_mus_to_midi;

    fn doom1_wad_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("doom1.wad")
    }

    #[test]
    #[ignore = "CI doesn't have a sound device"]
    fn play_midi_basic() {
        let wad = WadData::new(&doom1_wad_path());

        let lump = wad.get_lump("D_E1M8").unwrap();
        let res = read_mus_to_midi(&lump.data).unwrap();

        let sdl = sdl2::init().unwrap();
        let _audio = sdl.audio().unwrap();

        let frequency = 44_100;
        let format = AUDIO_S16LSB;
        let channels = DEFAULT_CHANNELS;
        let chunk_size = 1_024;
        sdl2::mixer::open_audio(frequency, format, channels, chunk_size).unwrap();
        let _mixer_context = sdl2::mixer::init(InitFlag::MOD).unwrap();

        sdl2::mixer::allocate_channels(16);

        let mut file = File::create("/tmp/doom.mid").unwrap();
        file.write_all(&res).unwrap();

        let music = sdl2::mixer::Music::from_file("/tmp/doom.mid").unwrap();

        println!("music => {:?}", music);
        println!("music type => {:?}", music.get_type());
        println!("music volume => {:?}", sdl2::mixer::Music::get_volume());
        println!("play => {:?}", music.play(1));

        std::thread::sleep(Duration::from_secs(10));
    }

    #[test]
    #[ignore = "CI doesn't have a sound device"]
    fn play_midi() {
        unsafe {
            set_var("SDL_MIXER_DISABLE_FLUIDSYNTH", "1");
            set_var("TIMIDITY_CFG", "/tmp/timidity.cfg");
        }
        let wad = WadData::new(&doom1_wad_path());

        let lump = wad.get_lump("D_E1M1").unwrap();
        let res = read_mus_to_midi(&lump.data).unwrap();

        let sdl = sdl2::init().unwrap();
        let _audio = sdl.audio().unwrap();

        let frequency = 44_100;
        let format = AUDIO_S16LSB;
        let channels = DEFAULT_CHANNELS;
        let chunk_size = 1_024;
        sdl2::mixer::open_audio(frequency, format, channels, chunk_size).unwrap();
        let _mixer_context = sdl2::mixer::init(InitFlag::MOD).unwrap();

        sdl2::mixer::allocate_channels(16);

        let mut file = File::create("/tmp/doom.mid").unwrap();
        file.write_all(&res).unwrap();

        let music = sdl2::mixer::Music::from_file("/tmp/doom.mid").unwrap();

        println!("music => {:?}", music);
        println!("music type => {:?}", music.get_type());
        println!("music volume => {:?}", sdl2::mixer::Music::get_volume());
        println!("play => {:?}", music.play(1));

        std::thread::sleep(Duration::from_secs(10));
    }
}
