use sound_traits::{SfxName, SoundAction, SoundServer, SoundServerTic};
use wad::WadData;

use crate::{MusicType, Snd};

#[test]
#[ignore = "SDL2 can only initialise once (and CI doesn't have sound)"]
fn play_weapons_snd() {
    let wad = WadData::new("../doom1.wad".into());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad, MusicType::Timidity).unwrap();
    let tx = snd.init().unwrap();

    let _thread = std::thread::spawn(move || {
        loop {
            snd.tic();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });

    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Pistol,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Shotgn,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Chgun,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Rlaunc,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    tx.send(SoundAction::Shutdown).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[test]
#[ignore = "SDL2 can only initialise once (and CI doesn't have sound)"]
fn play_demons_snd() {
    let wad = WadData::new("../doom1.wad".into());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad, MusicType::Timidity).unwrap();
    let tx = snd.init().unwrap();

    let _thread = std::thread::spawn(move || {
        loop {
            snd.tic();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });

    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Bgsit1,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Bgdth1,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxName::Posit2,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    tx.send(SoundAction::Shutdown).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[test]
#[ignore = "SDL2 can only initialise once (and CI doesn't have sound)"]
fn play_music() {
    let wad = WadData::new("../doom1.wad".into());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad, MusicType::OPL2).unwrap();
    let tx = snd.init().unwrap();

    let _thread = std::thread::spawn(move || {
        loop {
            snd.tic();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });

    tx.send(SoundAction::StartMusic(1, false)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    tx.send(SoundAction::Shutdown).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[test]
#[ignore = "SDL2 can only initialise once (and CI doesn't have sound)"]
fn play_opl2_music() {
    let wad = WadData::new("../doom1.wad".into());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad, MusicType::OPL2).unwrap();
    let tx = snd.init().unwrap();

    let _thread = std::thread::spawn(move || {
        loop {
            snd.tic();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });

    // Test playing E1M1 music with OPL2
    tx.send(SoundAction::StartMusic(1, true)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2000));

    // Test changing music
    tx.send(SoundAction::ChangeMusic(2, false)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));

    // Test volume control
    tx.send(SoundAction::MusicVolume(32)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));

    // Test pause/resume
    tx.send(SoundAction::PauseMusic).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    tx.send(SoundAction::ResumeMusic).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    tx.send(SoundAction::Shutdown).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
}
