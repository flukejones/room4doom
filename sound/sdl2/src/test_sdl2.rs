use std::thread;
use std::time::Duration;

use sound_common::{SfxName, SoundAction, SoundServer, SoundServerTic};
use test_utils::doom1_wad_path;
use wad::WadData;

use crate::mus2midi::read_mus_to_midi;
use crate::{MusicType, Snd};

fn load_e1m1_midi(wad: &WadData) -> Vec<u8> {
    let lump = wad.get_lump("D_E1M1").expect("D_E1M1 lump");
    read_mus_to_midi(&lump.data).expect("MUS to MIDI")
}

#[test]
#[ignore = "SDL2 can only initialise once (and CI doesn't have sound)"]
fn play_weapons_snd() {
    let wad = WadData::new(&doom1_wad_path());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad, MusicType::GUS).unwrap();
    let tx = snd.init().unwrap();

    let _thread = thread::spawn(move || {
        loop {
            snd.tic();
            thread::sleep(Duration::from_millis(5));
        }
    });

    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Pistol,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    thread::sleep(Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Shotgn,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    thread::sleep(Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Chgun,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    thread::sleep(Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Rlaunc,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    thread::sleep(Duration::from_millis(500));

    tx.send(SoundAction::Shutdown).unwrap();
    thread::sleep(Duration::from_millis(500));
}

#[test]
#[ignore = "SDL2 can only initialise once (and CI doesn't have sound)"]
fn play_demons_snd() {
    let wad = WadData::new(&doom1_wad_path());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad, MusicType::GUS).unwrap();
    let tx = snd.init().unwrap();

    let _thread = thread::spawn(move || {
        loop {
            snd.tic();
            thread::sleep(Duration::from_millis(5));
        }
    });

    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Bgsit1,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    thread::sleep(Duration::from_millis(500));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Bgdth1,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    thread::sleep(Duration::from_millis(300));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Posit2,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    thread::sleep(Duration::from_millis(500));

    tx.send(SoundAction::Shutdown).unwrap();
    thread::sleep(Duration::from_millis(500));
}

#[test]
#[ignore = "SDL2 can only initialise once (and CI doesn't have sound)"]
fn play_music() {
    let wad = WadData::new(&doom1_wad_path());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad, MusicType::OPL2).unwrap();
    let tx = snd.init().unwrap();

    let _thread = thread::spawn(move || {
        loop {
            snd.tic();
            thread::sleep(Duration::from_millis(5));
        }
    });

    tx.send(SoundAction::StartMusic(load_e1m1_midi(&wad), false))
        .unwrap();
    thread::sleep(Duration::from_millis(500));

    tx.send(SoundAction::Shutdown).unwrap();
    thread::sleep(Duration::from_millis(500));
}

#[test]
#[ignore = "SDL2 can only initialise once (and CI doesn't have sound)"]
fn play_opl2_music() {
    let wad = WadData::new(&doom1_wad_path());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad, MusicType::OPL2).unwrap();
    let tx = snd.init().unwrap();

    let _thread = thread::spawn(move || {
        loop {
            if !snd.tic() {
                return;
            }
        }
    });

    // Test playing E1M1 music with OPL2
    tx.send(SoundAction::MusicVolume(100)).unwrap();
    tx.send(SoundAction::StartMusic(load_e1m1_midi(&wad), true))
        .unwrap();
    thread::sleep(Duration::from_millis(5000));

    // // Test changing music
    // tx.send(SoundAction::ChangeMusic(1, false)).unwrap();
    // thread::sleep(Duration::from_millis(1000));

    // // Test volume control
    // thread::sleep(Duration::from_millis(5000));

    // Test pause/resume
    tx.send(SoundAction::PauseMusic).unwrap();
    thread::sleep(Duration::from_millis(500));
    tx.send(SoundAction::ResumeMusic).unwrap();
    thread::sleep(Duration::from_millis(500));

    tx.send(SoundAction::Shutdown).unwrap();
    thread::sleep(Duration::from_millis(500));
}
