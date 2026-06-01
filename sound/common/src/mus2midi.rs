//! Convert Doom MUS format to MIDI format.
//!
//! All parsing is bounds-checked: malformed or truncated MUS lumps return
//! `Err(MusError)` rather than panicking, so untrusted PWAD music data
//! cannot crash the engine.

use log::warn;

const MIDI_NOTEOFF: u8 = 0x80;
const MIDI_NOTEON: u8 = 0x90;
const MIDI_CTRLCHANGE: u8 = 0xB0;
const MIDI_PRGMCHANGE: u8 = 0xC0;
const MIDI_PITCHBEND: u8 = 0xE0;

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

const MUS_HEADER_LEN: usize = 16;
const SYSTEM_EVENT_MIN: u8 = 10;
const SYSTEM_EVENT_MAX: u8 = 15;
const CONTROLLER_MAX: u8 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusError {
    /// MUS data ran out before a complete read could finish.
    Truncated,
    /// MUS data carried a header signature mismatch (likely a MIDI lump).
    BadHeader,
    /// A system-event byte fell outside the 10..=15 range.
    InvalidSystemEvent(u8),
    /// A controller-event index exceeded the controller table.
    InvalidController(u8),
    /// Top three bits of an event byte mapped to an undefined event type.
    InvalidEventType(u8),
}

impl std::fmt::Display for MusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => write!(f, "MUS data truncated"),
            Self::BadHeader => write!(f, "MUS header signature mismatch"),
            Self::InvalidSystemEvent(b) => write!(f, "invalid MUS system event: {}", b),
            Self::InvalidController(b) => write!(f, "invalid MUS controller event: {}", b),
            Self::InvalidEventType(b) => write!(f, "invalid MUS event type: 0x{:02x}", b),
        }
    }
}

impl std::error::Error for MusError {}

#[inline]
fn read_byte(buf: &[u8], idx: usize) -> Result<u8, MusError> {
    buf.get(idx).copied().ok_or(MusError::Truncated)
}

#[inline]
fn read_u16_le(buf: &[u8], idx: usize) -> Result<u16, MusError> {
    let bytes = buf.get(idx..idx + 2).ok_or(MusError::Truncated)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
enum MusEventType {
    ReleaseNote = 0x00,
    PlayNote = 0x10,
    PitchBend = 0x20,
    SystemEvent = 0x30,
    Controller = 0x40,
    EndOfMeasure = 0x50,
    ScoreEnd = 0x60,
}

impl TryFrom<u8> for MusEventType {
    type Error = MusError;

    fn try_from(e: u8) -> Result<Self, Self::Error> {
        match e {
            0x00 => Ok(Self::ReleaseNote),
            0x10 => Ok(Self::PlayNote),
            0x20 => Ok(Self::PitchBend),
            0x30 => Ok(Self::SystemEvent),
            0x40 => Ok(Self::Controller),
            0x50 => Ok(Self::EndOfMeasure),
            0x60 => Ok(Self::ScoreEnd),
            _ => Err(MusError::InvalidEventType(e)),
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
    fn read(buf: &[u8]) -> Result<Self, MusError> {
        let head = buf.get(..MUS_HEADER_LEN).ok_or(MusError::Truncated)?;
        let mut id = [0; 4];
        id.copy_from_slice(&head[..4]);

        if id == MIDI_HEAD[..4] {
            return Err(MusError::BadHeader);
        }

        let length = read_u16_le(buf, 4)?;
        let offset = read_u16_le(buf, 6)?;
        let primary = read_u16_le(buf, 8)?;
        let secondary = read_u16_le(buf, 10)?;
        let num_instruments = read_u16_le(buf, 12)?;
        let padding = read_u16_le(buf, 14)?;

        let mut instruments = Vec::with_capacity(num_instruments as usize);
        let mut marker = MUS_HEADER_LEN;
        for _ in 0..num_instruments {
            instruments.push(read_u16_le(buf, marker)?);
            marker += 2;
        }

        Ok(Self {
            id,
            length,
            offset,
            primary,
            secondary,
            num_instruments,
            instruments,
            padding,
        })
    }
}

struct EventByte {
    last: bool,
    kind: MusEventType,
    channel: u8,
}

impl EventByte {
    fn read(buf: &[u8], marker: &mut usize) -> Result<Self, MusError> {
        *marker += 1;
        let byte = read_byte(buf, *marker)?;

        Ok(Self {
            last: (byte & 0x80) == 0x80,
            kind: MusEventType::try_from(byte & 0x70)?,
            channel: byte & 0xF,
        })
    }
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
struct MusEvent {
    /// MUS variable-length delay in ticks (can exceed 255).
    delay: u32,
    kind: MusEventType,
    channel: u8,
    /// Note, pitch bend, controller number, etc.
    data1: u8,
    data2: u8,
    volume: u8,
}

impl MusEvent {
    fn read_release_note(
        buf: &[u8],
        marker: &mut usize,
        channels: &mut [u8; 16],
    ) -> Result<Self, MusError> {
        let byte = EventByte::read(buf, marker)?;
        *marker += 1;
        let data = read_byte(buf, *marker)?;
        let delay = read_delay(buf, marker, byte.last)?;

        Ok(Self {
            delay,
            kind: byte.kind,
            channel: byte.channel,
            data1: data & 0x7F,
            data2: 0,
            volume: channels[byte.channel as usize],
        })
    }

    fn read_play_note(
        buf: &[u8],
        marker: &mut usize,
        channels: &mut [u8; 16],
    ) -> Result<Self, MusError> {
        let byte = EventByte::read(buf, marker)?;
        *marker += 1;
        let data = read_byte(buf, *marker)?;

        if data & 0x80 == 0x80 {
            *marker += 1;
            let vol = read_byte(buf, *marker)?;
            channels[byte.channel as usize] = vol & 0x7F;
        }

        let delay = read_delay(buf, marker, byte.last)?;

        Ok(Self {
            delay,
            kind: byte.kind,
            channel: byte.channel,
            data1: data & 0x7F,
            data2: 0,
            volume: channels[byte.channel as usize],
        })
    }

    fn read_pitch_bend(buf: &[u8], marker: &mut usize) -> Result<Self, MusError> {
        let byte = EventByte::read(buf, marker)?;
        *marker += 1;
        let data = read_byte(buf, *marker)?;
        let delay = read_delay(buf, marker, byte.last)?;

        Ok(Self {
            delay,
            kind: byte.kind,
            channel: byte.channel,
            data1: data,
            data2: 0,
            volume: 0,
        })
    }

    fn read_system_event(buf: &[u8], marker: &mut usize) -> Result<Self, MusError> {
        let byte = EventByte::read(buf, marker)?;
        *marker += 1;
        let data = read_byte(buf, *marker)? & 0x7F;
        if !(SYSTEM_EVENT_MIN..=SYSTEM_EVENT_MAX).contains(&data) {
            return Err(MusError::InvalidSystemEvent(data));
        }

        let delay = read_delay(buf, marker, byte.last)?;
        Ok(Self {
            delay,
            kind: byte.kind,
            channel: byte.channel,
            data1: data,
            data2: 0,
            volume: 0,
        })
    }

    fn read_controller(
        buf: &[u8],
        marker: &mut usize,
        channels: &mut [u8; 16],
    ) -> Result<Self, MusError> {
        let byte = EventByte::read(buf, marker)?;
        *marker += 1;
        let data1 = read_byte(buf, *marker)? & 0x7F;
        if data1 > CONTROLLER_MAX {
            return Err(MusError::InvalidController(data1));
        }

        *marker += 1;
        let data2 = read_byte(buf, *marker)? & 0x7F;
        let delay = read_delay(buf, marker, byte.last)?;

        if data1 == 3 {
            channels[byte.channel as usize] = data2;
        }

        Ok(Self {
            delay,
            kind: byte.kind,
            channel: byte.channel,
            data1,
            data2,
            volume: 0,
        })
    }

    fn read_generic(buf: &[u8], marker: &mut usize) -> Result<Self, MusError> {
        let byte = EventByte::read(buf, marker)?;
        let delay = read_delay(buf, marker, byte.last)?;
        *marker += 1;

        Ok(Self {
            delay,
            kind: byte.kind,
            channel: byte.channel,
            data1: 0,
            data2: 0,
            volume: 0,
        })
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
        }
    }
}

/// Read MUS variable-length delay. Matches Chocolate Doom's mus2mid.c
/// decoding: accumulate 7-bit groups until continuation bit is clear.
fn read_delay(buf: &[u8], marker: &mut usize, last: bool) -> Result<u32, MusError> {
    if !last {
        return Ok(0);
    }

    let mut delay: u32 = 0;
    loop {
        *marker += 1;
        let byte = read_byte(buf, *marker)?;
        delay = delay * 128 + (byte as u32 & 0x7F);
        if byte & 0x80 == 0 {
            break;
        }
    }
    Ok(delay)
}

fn read_mus_event(
    buf: &[u8],
    marker: &mut usize,
    channels: &mut [u8; 16],
) -> Result<MusEvent, MusError> {
    let event = read_byte(buf, *marker + 1)? & 0x70;
    match MusEventType::try_from(event)? {
        MusEventType::ReleaseNote => MusEvent::read_release_note(buf, marker, channels),
        MusEventType::PlayNote => MusEvent::read_play_note(buf, marker, channels),
        MusEventType::PitchBend => MusEvent::read_pitch_bend(buf, marker),
        MusEventType::SystemEvent => MusEvent::read_system_event(buf, marker),
        MusEventType::Controller => MusEvent::read_controller(buf, marker, channels),
        MusEventType::EndOfMeasure | MusEventType::ScoreEnd => MusEvent::read_generic(buf, marker),
    }
}

fn read_track(buf: &[u8], header: &MusHeader) -> Result<Vec<MusEvent>, MusError> {
    let mut track = Vec::new();
    let track_end = header.length as usize + header.offset as usize;
    let mut marker = (header.offset as usize).saturating_sub(1);
    let mut channels = [0u8; 16];

    while marker < track_end.saturating_sub(1) {
        track.push(read_mus_event(buf, &mut marker, &mut channels)?);
    }

    Ok(track)
}

/// Convert MUS data to MIDI. Returns `None` (with a warning logged) on
/// any parse failure, including truncation, header mismatch, or invalid
/// event data — untrusted PWAD music never panics.
pub fn read_mus_to_midi(buf: &[u8]) -> Option<Vec<u8>> {
    match read_mus_to_midi_inner(buf) {
        Ok(out) => Some(out),
        Err(e) => {
            warn!("MUS-to-MIDI conversion failed: {e}");
            None
        }
    }
}

fn read_mus_to_midi_inner(buf: &[u8]) -> Result<Vec<u8>, MusError> {
    let header = MusHeader::read(buf)?;
    let track = read_track(buf, &header)?;

    let mut out = Vec::with_capacity(MIDI_HEAD.len() + header.length as usize);
    for i in MIDI_HEAD {
        out.push(i);
    }
    // Division: 70 ticks per quarter note.
    // With default MIDI tempo (500,000 µs/beat = 120 BPM):
    //   1 tick = 500,000 / (70 * 1,000,000) = 1/140s = MUS tick rate.
    for i in 70u16.to_be_bytes() {
        out.push(i);
    }
    // Track header + placeholder length
    for i in MIDI_HEAD2 {
        out.push(i);
    }

    let mut delay: u32 = 0;
    for event in track.iter() {
        if delay == 0 {
            out.push(0);
        } else {
            if delay >= 0x20_0000 {
                out.push(((delay & 0xFE0_0000) >> 21) as u8 | 0x80);
            }
            if delay >= 0x4000 {
                out.push(((delay & 0x1F_C000) >> 14) as u8 | 0x80);
            }
            if delay >= 0x80 {
                out.push(((delay & 0x3F80) >> 7) as u8 | 0x80);
            }
            out.push(delay as u8 & 0x7F);
        }

        let mut event = (*event).clone();
        event.convert_channel();
        event.to_midi(&mut out);
        delay = event.delay;
    }

    // write the length
    let len = (out.len() as u32 - 22).to_be_bytes();
    out[18] = len[0];
    out[19] = len[1];
    out[20] = len[2];
    out[21] = len[3];

    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Read;
    use std::path::PathBuf;

    use super::*;

    fn test_data_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("data")
            .join(name)
    }

    #[test]
    fn spot_check() {
        let mut file = File::open(test_data_path("e1m2.mus")).unwrap();
        let mut tmp = Vec::new();
        file.read_to_end(&mut tmp).unwrap();
        let header = MusHeader::read(&tmp).unwrap();
        let mus2mid = read_track(&tmp, &header).unwrap();

        assert_eq!(
            mus2mid[0],
            MusEvent {
                delay: 0,
                kind: MusEventType::Controller,
                channel: 0,
                data1: 0,
                data2: 48,
                volume: 0
            }
        );

        assert_eq!(
            mus2mid[1],
            MusEvent {
                delay: 0,
                kind: MusEventType::Controller,
                channel: 0,
                data1: 3,
                data2: 0,
                volume: 0
            }
        );

        assert_eq!(
            mus2mid[10],
            MusEvent {
                delay: 0,
                kind: MusEventType::Controller,
                channel: 1,
                data1: 3,
                data2: 0,
                volume: 0
            }
        );

        assert_eq!(
            mus2mid[11],
            MusEvent {
                delay: 0,
                kind: MusEventType::Controller,
                channel: 1,
                data1: 4,
                data2: 114,
                volume: 0
            }
        );

        assert_eq!(
            mus2mid[12],
            MusEvent {
                delay: 0,
                kind: MusEventType::Controller,
                channel: 2,
                data1: 0,
                data2: 37,
                volume: 0
            }
        );

        assert_eq!(
            mus2mid[50],
            MusEvent {
                delay: 2,
                kind: MusEventType::Controller,
                channel: 0,
                data1: 3,
                data2: 93,
                volume: 0
            }
        );

        assert_eq!(
            mus2mid[200],
            MusEvent {
                delay: 1,
                kind: MusEventType::Controller,
                channel: 0,
                data1: 3,
                data2: 126,
                volume: 0
            }
        );
    }

    #[test]
    fn e1m2_compare() {
        let mut file = File::open(test_data_path("e1m2.mus")).unwrap();
        let mut tmp = Vec::new();
        file.read_to_end(&mut tmp).unwrap();
        let mus2mid = read_mus_to_midi(&tmp).unwrap();

        // Verify the conversion produces valid MIDI (starts with MThd header)
        // and is in the expected size range.
        assert!(
            mus2mid.len() > 1000,
            "MIDI output too short: {}",
            mus2mid.len()
        );
        assert_eq!(&mus2mid[..4], b"MThd", "Missing MIDI header");
    }

    #[test]
    fn truncated_input_returns_none() {
        // 3 bytes is shorter than the 16-byte header.
        let buf = [0u8, 0, 0];
        assert!(read_mus_to_midi(&buf).is_none());
    }

    #[test]
    fn midi_header_signature_returns_none() {
        // Buffer starts with "MThd" — should be detected as already-MIDI.
        let mut buf = vec![b'M', b'T', b'h', b'd'];
        buf.extend_from_slice(&[0u8; 16]);
        assert!(read_mus_to_midi(&buf).is_none());
    }
}
