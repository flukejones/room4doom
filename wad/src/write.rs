//! PWAD container serialization.
//!
//! ```text
//! header:     "PWAD" + lump count (i32 LE) + directory offset (i32 LE)
//! lump data:  concatenated lump bytes in list order
//! directory:  per lump: offset (i32 LE) + size (i32 LE) + name (8 bytes,
//!             NUL padded)
//! ```
//!
//! Marker lumps (map names, section markers) are zero-length entries; their
//! directory offset points at the current data position, matching vanilla
//! tool output.

use std::fmt;
use std::io;
use std::path::Path;

use crate::Lump;

/// Maximum bytes in a lump name; longer names are an error, shorter are
/// NUL-padded.
pub const LUMP_NAME_LEN: usize = 8;
const PWAD_MAGIC: &[u8; 4] = b"PWAD";
const HEADER_LEN: usize = 12;
const DIR_ENTRY_LEN: usize = 16;

/// Failure while serializing or saving a PWAD.
#[derive(Debug)]
pub enum WadWriteError {
    /// A lump name exceeds [`LUMP_NAME_LEN`] bytes.
    NameTooLong(String),
    Io(io::Error),
}

impl fmt::Display for WadWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NameTooLong(name) => {
                write!(f, "lump name longer than {LUMP_NAME_LEN} bytes: {name:?}")
            }
            Self::Io(e) => write!(f, "pwad io error: {e}"),
        }
    }
}

impl std::error::Error for WadWriteError {}

impl From<io::Error> for WadWriteError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Serialize lumps into a complete PWAD byte image (header + data +
/// directory).
pub fn write_pwad(lumps: &[Lump]) -> Result<Vec<u8>, WadWriteError> {
    for lump in lumps {
        if lump.name.len() > LUMP_NAME_LEN {
            return Err(WadWriteError::NameTooLong(lump.name.clone()));
        }
    }

    let data_len: usize = lumps.iter().map(|l| l.data.len()).sum();
    let mut out = Vec::with_capacity(HEADER_LEN + data_len + lumps.len() * DIR_ENTRY_LEN);

    out.extend_from_slice(PWAD_MAGIC);
    out.extend_from_slice(&(lumps.len() as i32).to_le_bytes());
    out.extend_from_slice(&((HEADER_LEN + data_len) as i32).to_le_bytes());

    for lump in lumps {
        out.extend_from_slice(&lump.data);
    }

    let mut offset = HEADER_LEN;
    for lump in lumps {
        out.extend_from_slice(&(offset as i32).to_le_bytes());
        out.extend_from_slice(&(lump.data.len() as i32).to_le_bytes());
        let mut name = [0u8; LUMP_NAME_LEN];
        name[..lump.name.len()].copy_from_slice(lump.name.as_bytes());
        out.extend_from_slice(&name);
        offset += lump.data.len();
    }

    Ok(out)
}

/// Serialize and write a PWAD file.
pub fn save_pwad(path: &Path, lumps: &[Lump]) -> Result<(), WadWriteError> {
    let bytes = write_pwad(lumps)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WadData;

    fn lump(name: &str, data: &[u8]) -> Lump {
        Lump {
            name: name.to_owned(),
            data: data.to_vec(),
        }
    }

    #[test]
    fn header_and_directory_layout() {
        let lumps = [lump("E1M1", &[]), lump("THINGS", &[1, 2, 3, 4])];
        let bytes = write_pwad(&lumps).expect("valid names serialize");

        assert_eq!(&bytes[0..4], b"PWAD");
        assert_eq!(
            i32::from_le_bytes(bytes[4..8].try_into().expect("4 bytes")),
            2
        );
        let dir_offset = i32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes")) as usize;
        assert_eq!(dir_offset, 16);
        assert_eq!(bytes.len(), 16 + 2 * DIR_ENTRY_LEN);

        let e1m1_off = i32::from_le_bytes(bytes[16..20].try_into().expect("4 bytes"));
        let e1m1_size = i32::from_le_bytes(bytes[20..24].try_into().expect("4 bytes"));
        assert_eq!((e1m1_off, e1m1_size), (12, 0));
        assert_eq!(&bytes[24..32], b"E1M1\0\0\0\0");

        let things_off = i32::from_le_bytes(bytes[32..36].try_into().expect("4 bytes"));
        let things_size = i32::from_le_bytes(bytes[36..40].try_into().expect("4 bytes"));
        assert_eq!((things_off, things_size), (12, 4));
        assert_eq!(&bytes[40..48], b"THINGS\0\0");
    }

    #[test]
    fn long_name_rejected() {
        let lumps = [lump("WAYTOOLONG", &[])];
        let err = write_pwad(&lumps).expect_err("10-byte name invalid");
        assert!(matches!(err, WadWriteError::NameTooLong(_)), "{err}");
    }

    #[test]
    fn written_pwad_reads_back_with_wad_crate() {
        let lumps = [
            lump("E1M1", &[]),
            lump("THINGS", &[9, 8, 7, 6, 5, 4, 3, 2, 1, 0]),
        ];
        let path = std::env::temp_dir().join("editor_wad_write_test.wad");
        save_pwad(&path, &lumps).expect("temp file writes");

        let wad = WadData::new(&path);
        assert!(wad.lump_exists("E1M1"));
        let things = wad.get_lump("THINGS").expect("THINGS exists");
        assert_eq!(things.data, lumps[1].data);

        std::fs::remove_file(&path).ok();
    }
}
