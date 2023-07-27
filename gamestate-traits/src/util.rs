use crate::MachinationTrait;
use render_traits::{PixelBuffer, RenderTarget};
use std::mem::MaybeUninit;
use wad::{
    lumps::{WadPatch, WAD_PATCH},
    WadData,
};

/// Pattern like `WINUM` or `STTNUM`
pub fn get_num_sprites(pattern: &str, start: usize, wad: &WadData) -> [WadPatch; 10] {
    let mut nums: [WadPatch; 10] = [WAD_PATCH; 10];
    for (i, num) in nums.iter_mut().enumerate() {
        let p = i + start;
        let lump = wad.get_lump(&format!("{pattern}{p}")).unwrap();
        *num = WadPatch::from_lump(lump);
    }
    nums
}

pub fn get_st_key_sprites(wad: &WadData) -> [WadPatch; 6] {
    let mut keys: [MaybeUninit<WadPatch>; 6] = [
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
        MaybeUninit::uninit(),
    ];
    for (i, key) in keys.iter_mut().enumerate() {
        let lump = wad.get_lump(&format!("STKEYS{i}")).unwrap();
        *key = MaybeUninit::new(WadPatch::from_lump(lump));
    }
    unsafe { keys.map(|n| n.assume_init()) }
}

pub fn draw_num_pixels(
    p: u32,
    mut x: i32,
    y: i32,
    pad: usize,
    nums: &[WadPatch],
    drawer: &impl MachinationTrait,
    pixels: &mut impl PixelBuffer,
) -> i32 {
    let f = (pixels.height() / 200) as i32;
    let width = nums[0].width as i32 * f;
    let digits: Vec<u32> = p
        .to_string()
        .chars()
        .map(|d| d.to_digit(10).unwrap())
        .collect();

    for n in digits.iter().rev() {
        x -= width;
        let num = &nums[*n as usize];
        drawer.draw_patch_pixels(num, x, y, pixels);
    }
    if digits.len() <= pad {
        for _ in 0..=pad - digits.len() {
            x -= width;
            drawer.draw_patch_pixels(&nums[0], x, y, pixels);
        }
    }

    x
}

pub fn draw_num(
    p: u32,
    x: i32,
    y: i32,
    pad: usize,
    nums: &[WadPatch],
    drawer: &impl MachinationTrait,
    buffer: &mut RenderTarget,
) -> i32 {
    // TODO: remove duplicated functionality
    match buffer.render_type() {
        render_traits::RenderType::Software => {
            let pixels = unsafe { buffer.software_unchecked() };
            draw_num_pixels(p, x, y, pad, nums, drawer, pixels)
        }
        render_traits::RenderType::SoftOpenGL => {
            let pixels = unsafe { buffer.soft_opengl_unchecked() };
            draw_num_pixels(p, x, y, pad, nums, drawer, pixels)
        }
        render_traits::RenderType::OpenGL => todo!(),
        render_traits::RenderType::Vulkan => todo!(),
    }
}
