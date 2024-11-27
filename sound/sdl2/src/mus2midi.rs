//! Convert Doom MUS format to MIDI format for use with e.g, SDL Music

const MIDI_NOTEOFF: u8 = 0x80; // + note + velocity
const MIDI_NOTEON: u8 = 0x90; // + note + velocity
const MIDI_CTRLCHANGE: u8 = 0xB0; // + ctrlr + value
const MIDI_PRGMCHANGE: u8 = 0xC0; // + new patch
const MIDI_PITCHBEND: u8 = 0xE0; // + pitch bend (2 bytes)

const MIDI_HEAD: [u8; 12] = [
    b'M', b'T', b'h', b'd', // Main header
    0x00, 0x00, 0x00, 0x06, // Header size
    0x00, 0x00, // MIDI type (0)
    0x00, 0x01, /* Number of tracks */
];

const MIDI_HEAD2: [u8; 8] = [
    b'M', b'T', b'r', b'k', // Start of track
    0x00, 0x00, 0x00, 0x00, /* Placeholder for track length */
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
    121, /* reset all controllers */
];

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
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

#[allow(unused)]
struct MusHeader {
    id: [u8; 4],
    length: u16,
    offset: u16,
    primary: u16,
    secondary: u16,
    num_instruments: u16,
    instruments: Vec<u16>,
    padding: u16,
}

impl MusHeader {
    fn read(buf: &[u8]) -> Option<Self> {
        let mut id = [0; 4];
        id.copy_from_slice(&buf[..4]);

        if id == MIDI_HEAD[..4] {
            return None;
        }

        let num_instruments = u16::from_le_bytes([buf[12], buf[13]]);
        let mut instruments = Vec::new();
        let mut marker = 16;
        for _ in 0..num_instruments {
            let n = u16::from_le_bytes([buf[marker], buf[marker + 1]]);
            instruments.push(n);
            marker += 2;
        }

        Some(Self {
            id,
            length: u16::from_le_bytes([buf[4], buf[5]]),
            offset: u16::from_le_bytes([buf[6], buf[7]]),
            primary: u16::from_le_bytes([buf[8], buf[9]]),
            secondary: u16::from_le_bytes([buf[10], buf[11]]),
            num_instruments,
            instruments,
            padding: u16::from_le_bytes([buf[14], buf[15]]),
        })
    }
}

struct EventByte {
    last: bool,
    kind: MusEventType,
    channel: u8,
}
impl EventByte {
    fn read(buf: &[u8], marker: &mut usize) -> Self {
        *marker += 1;
        let byte = buf[*marker];

        Self {
            last: (byte & 0x80) == 0x80,
            kind: MusEventType::from(byte & 0x70),
            channel: (byte & 0xF),
        }
    }
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
struct MusEvent {
    delay: u8,
    kind: MusEventType,
    channel: u8,
    /// not, pitch bend etc
    data1: u8,
    data2: u8,
    volume: u8,
}

impl MusEvent {
    fn read_release_note(buf: &[u8], marker: &mut usize, channels: &mut [u8; 16]) -> Self {
        let byte = EventByte::read(buf, marker);
        *marker += 1;
        let data = buf[*marker];
        let delay = read_delay(buf, marker, byte.last);

        Self {
            delay,
            kind: byte.kind,
            channel: byte.channel,
            data1: data & 0x7F,
            data2: 0,
            volume: channels[byte.channel as usize],
        }
    }

    fn read_play_note(buf: &[u8], marker: &mut usize, channels: &mut [u8; 16]) -> Self {
        let byte = EventByte::read(buf, marker);
        *marker += 1;
        let data = buf[*marker];

        if data & 0x80 == 0x80 {
            *marker += 1;
            // TODO: reverse the division once correct volume is found
            // Set base volume
            channels[byte.channel as usize] = (buf[*marker] & 0x7F) / 5;
        }

        let delay = read_delay(buf, marker, byte.last);

        Self {
            delay,
            kind: byte.kind,
            channel: byte.channel,
            data1: data & 0x7F,
            data2: 0,
            volume: channels[byte.channel as usize],
        }
    }

    fn read_pitch_bend(buf: &[u8], marker: &mut usize) -> Self {
        let byte = EventByte::read(buf, marker);
        *marker += 1;
        let data = buf[*marker];
        let delay = read_delay(buf, marker, byte.last);

        Self {
            delay,
            kind: byte.kind,
            channel: byte.channel,
            data1: data,
            data2: 0,
            volume: 0,
        }
    }

    fn read_system_event(buf: &[u8], marker: &mut usize) -> Self {
        let byte = EventByte::read(buf, marker);
        *marker += 1;
        let data = buf[*marker] & 0x7F;
        if !(10..=15).contains(&data) {
            panic!("MUS data contained invalid system event: {}", data);
        }

        let delay = read_delay(buf, marker, byte.last);
        Self {
            delay,
            kind: byte.kind,
            channel: byte.channel,
            data1: data,
            data2: 0,
            volume: 0,
        }
    }

    fn read_controller(buf: &[u8], marker: &mut usize, channels: &mut [u8; 16]) -> Self {
        let byte = EventByte::read(buf, marker);
        *marker += 1;
        let data1 = buf[*marker] & 0x7F;
        if data1 > 9 {
            panic!("MUS data contained invalid controller event: {}", data1);
        }

        *marker += 1;
        let data2 = buf[*marker] & 0x7F;
        let delay = read_delay(buf, marker, byte.last);

        if data1 == 3 {
            channels[byte.channel as usize] = data2;
        }

        Self {
            delay,
            kind: byte.kind,
            channel: byte.channel,
            data1,
            data2,
            volume: 0,
        }
    }

    fn read_generic(buf: &[u8], marker: &mut usize) -> Self {
        let byte = EventByte::read(buf, marker);
        let delay = read_delay(buf, marker, byte.last);
        *marker += 1;

        Self {
            delay,
            kind: byte.kind,
            channel: byte.channel,
            data1: 0,
            data2: 0,
            volume: 0,
        }
    }

    fn convert_channel(&mut self) {
        if self.channel == 15 {
            self.channel = 9
        }
    }

    fn to_midi(&self, out: &mut Vec<u8>) {
        match self.kind {
            MusEventType::ReleaseNote => {
                out.push(MIDI_NOTEOFF | self.channel);
                out.push(self.data1);
                out.push(self.volume);
            }
            MusEventType::PlayNote => {
                out.push(MIDI_NOTEON | self.channel);
                out.push(self.data1);
                out.push(self.volume);
            }
            MusEventType::PitchBend => {
                let msb = self.data1 >> 1;
                let lsb = (self.data1 << 7) & 0x80;
                out.push(MIDI_PITCHBEND | self.channel);
                out.push(lsb);
                out.push(msb);
            }
            MusEventType::SystemEvent => {
                out.push(MIDI_CTRLCHANGE | self.channel);
                out.push(TRANSLATE[self.data1 as usize]);
                out.push(self.data2);
            }
            MusEventType::Controller => {
                let controller = self.data1;
                if controller == 0 {
                    out.push(MIDI_PRGMCHANGE | self.channel);
                    out.push(self.data2);
                } else {
                    out.push(MIDI_CTRLCHANGE | self.channel);
                    out.push(TRANSLATE[self.data1 as usize]);
                    out.push(self.data2);
                }
            }
            MusEventType::ScoreEnd => {
                out.push(0xFF);
                out.push(0x2F);
                out.push(0);
            }
            MusEventType::EndOfMeasure => {}
            MusEventType::Unused => todo!(),
        }
    }
}

fn read_delay(buf: &[u8], marker: &mut usize, last: bool) -> u8 {
    if !last {
        return 0;
    }

    let mut byte = 0x80;
    let mut delay = 0;
    while byte & 0x80 != 0 {
        *marker += 1;
        byte = buf[*marker];
        delay = (delay as u16 * 128 + (byte as u16 & 0x7F)) as u8;
    }
    delay
}

fn read_mus_event(buf: &[u8], marker: &mut usize, channels: &mut [u8; 16]) -> MusEvent {
    // Decide which function to call with this event type
    let event = buf[*marker + 1] & 0x70;
    match MusEventType::from(event) {
        MusEventType::ReleaseNote => MusEvent::read_release_note(buf, marker, channels),
        MusEventType::PlayNote => MusEvent::read_play_note(buf, marker, channels),
        MusEventType::PitchBend => MusEvent::read_pitch_bend(buf, marker),
        MusEventType::SystemEvent => MusEvent::read_system_event(buf, marker),
        MusEventType::Controller => MusEvent::read_controller(buf, marker, channels),
        MusEventType::EndOfMeasure | MusEventType::ScoreEnd => MusEvent::read_generic(buf, marker),
        MusEventType::Unused => panic!("MUS event was some sort of invalid data"),
    }
}

fn read_track(buf: &[u8], header: &MusHeader) -> Vec<MusEvent> {
    let mut track = Vec::new();
    let mut marker = header.offset as usize - 1;
    let mut channels = [0u8; 16];

    for _ in header.offset..header.length + header.offset {
        if marker >= (header.length + header.offset) as usize - 1 {
            break;
        }
        let res = read_mus_event(buf, &mut marker, &mut channels);
        track.push(res);
    }

    track
}

/// Take an array of MUS data and convert directly to an array of MIDI data
pub fn read_mus_to_midi(buf: &[u8]) -> Option<Vec<u8>> {
    let header = MusHeader::read(buf)?;
    let track = read_track(buf, &header);

    let mut out = Vec::with_capacity(MIDI_HEAD.len() + header.length as usize);
    for i in MIDI_HEAD {
        out.push(i);
    }
    // timestamp
    for i in 560u16.to_be_bytes() {
        out.push(i);
    }
    // signature + length
    for i in MIDI_HEAD2 {
        out.push(i);
    }
    // tempo
    let tmp = [0, 0xFF, 0x51, 0x03, 0x0F, 0x42, 0x40];
    for i in tmp {
        out.push(i);
    }

    let mut delay = 0;
    for event in track.iter() {
        if delay == 0 {
            out.push(0);
        } else {
            // Original implementation of this used two loops, one to first build
            // up a u32 "buffer", then a second loop to do a similar bitshift.
            let tmp_delay = (delay as u32) * 4;
            if tmp_delay >= 0x20_0000 {
                out.push(((tmp_delay & 0xFE0_0000) >> 21) as u8 | 0x80);
            }
            if tmp_delay >= 0x4000 {
                out.push(((tmp_delay & 0x1F_C000) >> 14) as u8 | 0x80);
            }
            if tmp_delay >= 0x80 {
                out.push(((tmp_delay & 0x3F80) >> 7) as u8 | 0x80);
            }
            out.push(tmp_delay as u8 & 0x7F);
        }

        // write the event
        let mut event = (*event).clone();
        event.convert_channel();
        event.to_midi(&mut out);
        //
        delay = event.delay;
    }

    // write the length
    let len = (out.len() as u32 - 22).to_be_bytes();
    out[18] = len[0];
    out[19] = len[1];
    out[20] = len[2];
    out[21] = len[3];

    Some(out)
}

#[cfg(test)]
mod tests {
    use std::env::set_var;
    use std::fs::File;
    use std::io::{Read, Write};
    use std::time::Duration;

    use sdl2::mixer::{AUDIO_S16LSB, DEFAULT_CHANNELS, InitFlag};
    use wad::WadData;

    use crate::mus2midi::{MusEvent, MusEventType, MusHeader, read_track};

    use super::read_mus_to_midi;

    #[test]
    fn spot_check() {
        let mut file = File::open("data/e1m2.mus").unwrap();
        let mut tmp = Vec::new();
        file.read_to_end(&mut tmp).unwrap();
        let header = MusHeader::read(&tmp).unwrap();
        let mus2mid = read_track(&tmp, &header);

        assert_eq!(mus2mid[0], MusEvent {
            delay: 0,
            kind: MusEventType::Controller,
            channel: 0,
            data1: 0,
            data2: 48,
            volume: 0
        });

        assert_eq!(mus2mid[1], MusEvent {
            delay: 0,
            kind: MusEventType::Controller,
            channel: 0,
            data1: 3,
            data2: 0,
            volume: 0
        });

        assert_eq!(mus2mid[10], MusEvent {
            delay: 0,
            kind: MusEventType::Controller,
            channel: 1,
            data1: 3,
            data2: 0,
            volume: 0
        });

        assert_eq!(mus2mid[11], MusEvent {
            delay: 0,
            kind: MusEventType::Controller,
            channel: 1,
            data1: 4,
            data2: 114,
            volume: 0
        });

        assert_eq!(mus2mid[12], MusEvent {
            delay: 0,
            kind: MusEventType::Controller,
            channel: 2,
            data1: 0,
            data2: 37,
            volume: 0
        });

        assert_eq!(mus2mid[50], MusEvent {
            delay: 2,
            kind: MusEventType::Controller,
            channel: 0,
            data1: 3,
            data2: 93,
            volume: 0
        });

        assert_eq!(mus2mid[200], MusEvent {
            delay: 1,
            kind: MusEventType::Controller,
            channel: 0,
            data1: 3,
            data2: 126,
            volume: 0
        });
    }

    #[test]
    fn e1m2_compare() {
        let mut file = File::open("data/e1m2.mus").unwrap();
        let mut tmp = Vec::new();
        file.read_to_end(&mut tmp).unwrap();
        let mus2mid = read_mus_to_midi(&tmp).unwrap();

        let mut file = File::open("data/e1m2.mid").unwrap();
        let mut e1m2 = Vec::new();
        file.read_to_end(&mut e1m2).unwrap();

        assert_eq!(mus2mid.len(), e1m2.len());
        assert_eq!(mus2mid[112], e1m2[112]);
        assert_eq!(mus2mid[140], e1m2[140]);
        assert_eq!(mus2mid[2833], e1m2[2833]);
    }

    #[test]
    #[ignore = "CI doesn't have a sound device"]
    fn play_midi_basic() {
        let wad = WadData::new("../doom1.wad".into());

        let lump = wad.get_lump("D_E1M8").unwrap();
        let res = read_mus_to_midi(&lump.data).unwrap();

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
        let wad = WadData::new("../doom1.wad".into());

        let lump = wad.get_lump("D_E1M1").unwrap();
        let res = read_mus_to_midi(&lump.data).unwrap();

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
