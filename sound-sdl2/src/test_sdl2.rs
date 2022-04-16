use std::{f32::consts::PI, sync::atomic::Ordering};

use sound_traits::{SfxEnum, SoundAction, SoundObjPosition, SoundServer, SoundServerTic};
use wad::WadData;

use crate::Snd;

#[test]
fn run_tic() {
    let wad = WadData::new("../doom1.wad".into());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad).unwrap();
    let (tx, kill) = snd.init().unwrap();

    let _thread = std::thread::spawn(move || loop {
        snd.tic();
        std::thread::sleep(std::time::Duration::from_millis(5));
    });

    tx.send(SoundAction::StartSfx {
        origin: SoundObjPosition::new(123, (0.3, 0.3), PI),
        sfx: SfxEnum::bfg,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(333));

    tx.send(SoundAction::StartSfx {
        origin: SoundObjPosition::new(123, (0.3, 0.3), PI),

        sfx: SfxEnum::barexp,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(333));

    tx.send(SoundAction::UpdateSound {
        listener: SoundObjPosition::new(123, (1.0, 1.0), PI / 2.0),
    })
    .unwrap();

    tx.send(SoundAction::StopSfx { uid: 456 }).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));

    kill.store(true, Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[test]
fn play_weapons_snd() {
    let wad = WadData::new("../doom1.wad".into());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad).unwrap();
    let (tx, kill) = snd.init().unwrap();

    let _thread = std::thread::spawn(move || loop {
        snd.tic();
        std::thread::sleep(std::time::Duration::from_millis(5));
    });

    tx.send(SoundAction::StartSfx {
        origin: SoundObjPosition::new(123, (0.3, 0.3), PI),

        sfx: SfxEnum::pistol,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        origin: SoundObjPosition::new(123, (0.3, 0.3), PI),

        sfx: SfxEnum::shotgn,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        origin: SoundObjPosition::new(123, (0.3, 0.3), PI),

        sfx: SfxEnum::chgun,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        origin: SoundObjPosition::new(123, (0.3, 0.3), PI),

        sfx: SfxEnum::rlaunc,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    kill.store(true, Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[test]
fn play_demons_snd() {
    let wad = WadData::new("../doom1.wad".into());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad).unwrap();
    let (tx, kill) = snd.init().unwrap();

    let _thread = std::thread::spawn(move || loop {
        snd.tic();
        std::thread::sleep(std::time::Duration::from_millis(5));
    });

    tx.send(SoundAction::StartSfx {
        origin: SoundObjPosition::new(123, (0.3, 0.3), PI),

        sfx: SfxEnum::bgsit2,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    tx.send(SoundAction::StartSfx {
        origin: SoundObjPosition::new(123, (0.3, 0.3), PI),

        sfx: SfxEnum::bgdth1,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    tx.send(SoundAction::StartSfx {
        origin: SoundObjPosition::new(123, (0.3, 0.3), PI),

        sfx: SfxEnum::posit2,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    kill.store(true, Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[test]
fn play_music() {
    let wad = WadData::new("../doom1.wad".into());
    let sdl = sdl2::init().unwrap();

    let mut snd = Snd::new(sdl.audio().unwrap(), &wad).unwrap();
    let (tx, kill) = snd.init().unwrap();

    let _thread = std::thread::spawn(move || loop {
        snd.tic();
        std::thread::sleep(std::time::Duration::from_millis(5));
    });

    tx.send(SoundAction::StartMusic(1, false)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    kill.store(true, Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(500));
}
