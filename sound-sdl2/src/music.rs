const MIDI_SYSEX: u8 = 0xF0; // SysEx begin
const MIDI_SYSEXEND: u8 = 0xF7; // SysEx end
const MIDI_META: u8 = 0xFF; // Meta event begin
const MIDI_META_TEMPO: u8 = 0x51;
const MIDI_META_EOT: u8 = 0x2F; // End-of-track
const MIDI_META_SSPEC: u8 = 0x7F; // System-specific event

const MIDI_NOTEOFF: u8 = 0x80; // + note + velocity
const MIDI_NOTEON: u8 = 0x90; // + note + velocity
const MIDI_POLYPRESS: u8 = 0xA0; // + pressure (2 bytes)
const MIDI_CTRLCHANGE: u8 = 0xB0; // + ctrlr + value
const MIDI_PRGMCHANGE: u8 = 0xC0; // + new patch
const MIDI_CHANPRESS: u8 = 0xD0; // + pressure (1 byte)
const MIDI_PITCHBEND: u8 = 0xE0; // + pitch bend (2 bytes)

const MIDI_HEAD: [u8; 22] = [
    b'M', b'T', b'h', b'd', // Main header
    0x00, 0x00, 0x00, 0x06, // Header size
    0x00, 0x00,             // MIDI type (0)
    0x00, 0x01,             // Number of tracks
    0x00, 0x46,             // Resolution
    b'M', b'T', b'r', b'k',  // Start of track
    0x00, 0x00, 0x00, 0x00  // Placeholder for track length
];

const TRANSLATE: [u8; 15] = [
    0,   // program change
    0,   // bank select
    1,   // modulation pot
    7,   // volume
    10,  // pan pot
    11,  // expression pot
    91,  // reverb depth
    93,  // chorus depth
    64,  // sustain pedal
    67,  // soft pedal
    120, // all sounds off
    123, // all notes off
    126, // mono
    127, // poly
    121, // reset all controllers
];

fn read_var_len(buf: &[u8], time_out: &mut i32) -> usize {
    let mut time: u8 = 0;
    let mut ofs = 0;
    let mut t;

    loop {
        ofs += 1;
        t = buf[ofs];
        time = (time << 7) | (t & 127);

        if t & 128 == 0 {
            break;
        }
    }
    *time_out = time as i32;
    return ofs;
}

// Pushes on top of existing
fn write_var_len(out: &mut Vec<u8>, mut time: i32) -> i32 {
    let mut buffer: i32 = time & 0x7f;
    loop {
        time >>= 7;
        if time == 0 {
            break;
        }
            buffer = (buffer << 8) | ((time & 0x7f) | 0x80);
    }

    let mut ofs = 0;
    loop {
        out.push((buffer & 0xff) as u8);
        if buffer & 0x80 != 0 {
            buffer >>= 8;
        } else {
            break;
        }
        ofs += 1;
    }
    return ofs;
}

#[derive(Debug)]
enum MusEventType {
    ReleaseNote = 0x00,
    PlayNote = 0x10,
    PitchBend = 0x20,
    SystemEvent = 0x30,
    Controller = 0x40,
    EndOfMeasure = 0x50,
    ScoreEnd = 0x60,
    Unused,
}

impl From<u8> for MusEventType {
    fn from(e: u8) -> Self {
        match e {
            0x00 => MusEventType::ReleaseNote,
            0x10 => MusEventType::PlayNote,
            0x20 => MusEventType::PitchBend,
            0x30 => MusEventType::SystemEvent,
            0x40 => MusEventType::Controller,
            0x50 => MusEventType::EndOfMeasure,
            0x60 => MusEventType::ScoreEnd,
            _ => MusEventType::Unused,
        }
    }
}

struct MusHeader {
    id: [u8; 4],
    length: u16,
    offset: u16,
    primary: u16,
    secondary: u16,
    num_instruments: u16,
    padding: u16,
}

impl MusHeader {
    fn read_header(buf: &[u8]) -> Self {
        let mut id = [0; 4];
        id.copy_from_slice(&buf[..4]);

        Self {
            id,
            length: u16::from_le_bytes([buf[4], buf[5]]),
            offset: u16::from_le_bytes([buf[6], buf[7]]),
            primary: u16::from_le_bytes([buf[8], buf[9]]),
            secondary: u16::from_le_bytes([buf[10], buf[11]]),
            num_instruments: u16::from_le_bytes([buf[12], buf[13]]),
            padding: u16::from_le_bytes([buf[14], buf[15]]),
        }
    }
}

fn convert_mus(buf: &[u8], header: &MusHeader) -> Vec<u8> {
    let mut marker = header.offset as usize;
    let mut track = Vec::with_capacity(MIDI_HEAD.len() + header.length as usize);
    for i in MIDI_HEAD {
        track.push(i);
    }

    let mut mid1 = 0;
    let mut mid2 = 0;
    let mut mid_status;
    let mut mid_args;
    let mut no_op = false;

    let mut channel_used = [0u8; 16];
    let mut last_vel = [100u8; 16];

    let mut event = 0;
    let mut status = 0;
    let mut delta_time = 0;
    for _ in header.offset..(header.offset + header.length) {
        dbg!(marker,event & 0x70);
        if event & 0x70 == MusEventType::ScoreEnd as u8
        {
            break;
        }

        marker += 1;
        event = buf[marker];

        let mut t = 0;
        if event & 0x70 != MusEventType::ScoreEnd as u8 {
            marker += 1;
            t = buf[marker];
        }

        let mut channel = event & 15;
        if channel == 15 {
            channel = 9;
        } else if channel >= 9 {
            channel += 1;
        }

        if channel_used[channel as usize] == 0 {
            // This is the first time this channel has been used,
            // so sets its volume to 127.
            channel_used[channel as usize] = 1;
            track.push(0);
            track.push(0xB0 | channel);
            track.push(7);
            track.push(127);
        }

        mid_status = channel;
        mid_args = 0;

        match MusEventType::from(event & 0x70) {
            MusEventType::ReleaseNote => {
                mid_status |= MIDI_NOTEOFF;
                mid1 = t & 127;
                mid2 = 64;
            }
            MusEventType::PlayNote => {
                mid_status |= MIDI_NOTEON;
                mid1 = t & 127;
                if (t & 128) != 0 {
                    marker += 1;
                    last_vel[channel as usize] = buf[marker] & 127;
                }
                mid2 = last_vel[channel as usize];
            }
            MusEventType::PitchBend => {
                mid_status |= MIDI_PITCHBEND;
                mid1 = (t & 1) << 6;
                mid2 = (t >> 1) & 127;
            }

            MusEventType::Controller => {
                if t == 0 {
                    // program change
                    mid_args = 1;
                    mid_status |= MIDI_PRGMCHANGE;
                    marker += 1;
                    mid1 = buf[marker] & 127;
                    mid2 = 0;
                } else if t > 0 && t < 10 {
                    mid_status |= MIDI_CTRLCHANGE;
                    mid1 = TRANSLATE[t as usize];
                    marker += 1;
                    mid2 = buf[marker];
                } else {
                    no_op = true;
                }
            }

            MusEventType::ScoreEnd => {
                mid_status = MIDI_META;
                mid1 = MIDI_META_EOT;
                mid2 = 0;
            }
            MusEventType::SystemEvent => {
                if t < 10 || t > 14 {
                    no_op = true;
                } else {
                    mid_status |= MIDI_CTRLCHANGE;
                    mid1 = TRANSLATE[t as usize];
                    mid2 = 12;
                    t = 12;
                }
            },
            MusEventType::EndOfMeasure => {},
            MusEventType::Unused => {}
        }

        if no_op {
            mid_status = MIDI_META;
            mid1 = MIDI_META_SSPEC;
            mid2 = 0;
        }

        write_var_len(&mut track, delta_time);
        dbg!(delta_time);

        if mid_status != status {
            status = mid_status;
            track.push(status);
        }

        track.push(mid1);

        if mid_args == 0 {
            track.push(mid2);
        }
        if event & 128 != 0 {
            marker += read_var_len(&track[marker..], &mut delta_time);
        } else {
            delta_time = 0;
        }
    }

    let len = track.len() as u32 - 22;
    track[18] = ((len >> 24) & 0xff) as u8;
    track[19] = ((len >> 16) & 0xff) as u8;
    track[20] = ((len >> 8) & 0xff) as u8;
    track[21] = (len & 0xff) as u8;
    track
}

pub fn read_mus(buf: &[u8]) -> Vec<u8> {
    let header = MusHeader::read_header(buf);
    convert_mus(buf, &header)
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write, time::Duration};

    use sdl2::mixer::{InitFlag, AUDIO_S16LSB, DEFAULT_CHANNELS};
    use wad::WadData;

    use crate::music::MusHeader;

    use super::read_mus;

    #[test]
    fn mus2midi() {
        let wad = WadData::new("../doom1.wad".into());

        let lump = wad.get_lump("D_E1M1").unwrap();

        let header = MusHeader::read_header(&lump.data);
        assert_eq!(header.id[0], b'M');
        assert_eq!(header.id[1], b'U');
        assert_eq!(header.id[2], b'S');

        assert_eq!(header.length, 17334);
        // assert_eq!(header.length, 17237); // Doom registered

        let res = read_mus(&lump.data);
        dbg!(&res[18..22]);
    }

    #[test]
    fn play_midi() {
        let wad = WadData::new("../doom1.wad".into());

        let lump = wad.get_lump("D_E1M1").unwrap();
        let res = read_mus(&lump.data);

        let sdl = sdl2::init().unwrap();
        let _audio = sdl.audio().unwrap();

        let frequency = 44_100;
        let format = AUDIO_S16LSB; // signed 16 bit samples, in little-endian byte order
        let channels = DEFAULT_CHANNELS; // Stereo
        let chunk_size = 1_024;
        sdl2::mixer::open_audio(frequency, format, channels, chunk_size).unwrap();
        let _mixer_context = sdl2::mixer::init(InitFlag::MOD).unwrap();

        // Number of mixing channels available for sound effect `Chunk`s to play
        // simultaneously.
        sdl2::mixer::allocate_channels(4);

        let mut file = File::create("tmp.mid").unwrap();
        file.write_all(&res).unwrap();

        let music = sdl2::mixer::Music::from_file("tmp.mid").unwrap();

        println!("music => {:?}", music);
        println!("music type => {:?}", music.get_type());
        println!("music volume => {:?}", sdl2::mixer::Music::get_volume());
        println!("play => {:?}", music.play(1));

        std::thread::sleep(Duration::from_secs(3));
    }
}
