use crate::MachinationTrait;
use render_traits::PixelBuf;
use std::mem::MaybeUninit;
use wad::{lumps::WadPatch, WadData};

/// Pattern like `WINUM` or `STTNUM`
pub fn get_num_sprites(pattern: &str, start: usize, wad: &WadData) -> [WadPatch; 10] {
    let mut nums: [MaybeUninit<WadPatch>; 10] = [
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
    ];
    for n in 0..10 {
        let p = n + start;
        let lump = wad.get_lump(&format!("{pattern}{p}")).unwrap();
        nums[n] = MaybeUninit::new(WadPatch::from_lump(lump));
    }
    unsafe { nums.map(|n| n.assume_init()) }
}

pub fn get_st_key_sprites(wad: &WadData) -> [WadPatch; 6] {
    let mut nums: [MaybeUninit<WadPatch>; 6] = [
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
    ];
    for n in 0..6 {
        let lump = wad.get_lump(&format!("STKEYS{n}")).unwrap();
        nums[n] = MaybeUninit::new(WadPatch::from_lump(lump));
    }
    unsafe { nums.map(|n| n.assume_init()) }
}

pub fn draw_num(
    p: u32,
    mut x: i32,
    y: i32,
    pad: usize,
    nums: &[WadPatch],
    drawer: &impl MachinationTrait,
    buffer: &mut PixelBuf,
) -> i32 {
    let width = nums[0].width as i32;
    let digits: Vec<u32> = p
        .to_string()
        .chars()
        .map(|d| d.to_digit(10).unwrap())
        .collect();

    for n in digits.iter().rev() {
        x -= width;
        let num = &nums[*n as usize];
        drawer.draw_patch(num, x, y, buffer);
    }
    if digits.len() <= pad {
        for _ in 0..=pad - digits.len() {
            x -= width;
            drawer.draw_patch(&nums[0], x, y, buffer);
        }
    }

    x
}
