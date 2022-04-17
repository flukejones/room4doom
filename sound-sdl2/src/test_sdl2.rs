use std::{f32::consts::PI, sync::atomic::Ordering};

use sound_traits::{SfxEnum, SoundAction, SoundServer, SoundServerTic};
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
        uid: 123,
        sfx: SfxEnum::bfg,
        x: 0.3,
        y: 0.3,
        angle: PI,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(333));

    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::barexp,
        x: 0.3,
        y: 0.3,
        angle: PI,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(333));

    tx.send(SoundAction::UpdateListener {
        x: 0.3,
        y: 0.3,
        angle: PI / 2.0,
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
        uid: 123,
        sfx: SfxEnum::pistol,
        x: 0.3,
        y: 0.3,
        angle: PI,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::shotgn,
        x: 0.3,
        y: 0.3,
        angle: PI,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::chgun,
        x: 0.3,
        y: 0.3,
        angle: PI,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::rlaunc,
        x: 0.3,
        y: 0.3,
        angle: PI,
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
        uid: 123,
        sfx: SfxEnum::bgsit1,
        x: 0.3,
        y: 0.3,
        angle: PI,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::bgdth1,
        x: 0.3,
        y: 0.3,
        angle: PI,
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    tx.send(SoundAction::StartSfx {
        uid: 123,
        sfx: SfxEnum::posit2,
        x: 0.3,
        y: 0.3,
        angle: PI,
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
