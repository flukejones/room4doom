use sdl2::{
    audio::{AudioCVT, AudioFormat},
    mixer::Chunk,
};

#[cfg(test)]
mod test_sdl2;

/// `to_fmt` is almost always going to be `AudioFormat::S16LSB`, `to_rate` typically `44_100`.
pub fn lump_sfx_to_chunk(
    raw_lump: Vec<u8>,
    to_fmt: AudioFormat,
    to_rate: i32,
) -> Result<Chunk, String> {
    let rate = i16::from_le_bytes([raw_lump[2], raw_lump[3]]) as i32;
    let len = i32::from_le_bytes([raw_lump[4], raw_lump[5], raw_lump[6], raw_lump[7]]);
    let converter = AudioCVT::new(AudioFormat::U8, 1, rate, to_fmt, 2, to_rate)?;
    let fixed = converter.convert(raw_lump[7..len as usize].to_vec());

    sdl2::mixer::Chunk::from_raw_buffer(fixed.into_boxed_slice())
}
