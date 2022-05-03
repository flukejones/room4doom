use sound_traits::{SfxEnum, SoundAction, SoundServer, SoundServerTic};
use wad::WadData;

use crate::Snd;

#[ignore = "SDL2 can only initialise once (and CI doesn't have sound)"]
#[test]
fn play_weapons_snd() {
    let wad = WadData::new("../doom1.wad".into());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad).unwrap();
    let tx = snd.init().unwrap();

    let _thread = std::thread::spawn(move || loop {
        snd.tic();
        std::thread::sleep(std::time::Duration::from_millis(5));
    });

    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::Pistol,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::Shotgn,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::Chgun,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::Rlaunc,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    tx.send(SoundAction::Shutdown).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[ignore = "SDL2 can only initialise once (and CI doesn't have sound)"]
#[test]
fn play_demons_snd() {
    let wad = WadData::new("../doom1.wad".into());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad).unwrap();
    let tx = snd.init().unwrap();

    let _thread = std::thread::spawn(move || loop {
        snd.tic();
        std::thread::sleep(std::time::Duration::from_millis(5));
    });

    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::Bgsit1,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::Bgdth1,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::Posit2,
        x: 0.3,
        y: 0.3,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    tx.send(SoundAction::Shutdown).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[ignore = "SDL2 can only initialise once (and CI doesn't have sound)"]
#[test]
fn play_music() {
    let wad = WadData::new("../doom1.wad".into());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad).unwrap();
    let tx = snd.init().unwrap();

    let _thread = std::thread::spawn(move || loop {
        snd.tic();
        std::thread::sleep(std::time::Duration::from_millis(5));
    });

    tx.send(SoundAction::StartMusic(1, false)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    tx.send(SoundAction::Shutdown).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
}
