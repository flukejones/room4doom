//! Shared traits, error types, file format definitions, and the [`RenderPvs`]
//! bitset type for the PVS system.
//!
//! # File format
//!
//! ```text
//! Offset  Size  Field
//! 0       4     Magic   — e.g. b"PV2D"
//! 4       1     Version — u8, currently 1
//! 5       3     Reserved — [0u8; 3]
//! 8       8     subsector_count — u64 little-endian
//! 16      8     data_len — number of u32 words, u64 little-endian
//! 24      var   visibility data — u32 words, little-endian
//! ```

use super::portal::Portals;
use std::io::{self, Read};
use std::path::PathBuf;

// ============================================================================
// ERROR TYPE
// ============================================================================

/// Errors that can occur when loading or validating a PVS cache file.
#[derive(Debug)]
pub enum PvsFileError {
    /// Underlying I/O error.
    Io(io::Error),
    /// The file's magic bytes did not match the expected value.
    InvalidMagic([u8; 4]),
    /// The file was written by a newer version of the format than this build
    /// supports.
    VersionTooNew {
        /// Version found in the file.
        got: u8,
        /// Maximum version this build can read.
        max: u8,
    },
    /// The file's subsector count does not match the map being loaded.
    SubsectorCountMismatch {
        /// Subsector count the caller expected.
        expected: usize,
        /// Subsector count recorded in the file.
        got: usize,
    },
    /// The file is shorter than the minimum header size.
    TruncatedHeader,
}

impl std::fmt::Display for PvsFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PvsFileError::Io(e) => write!(f, "PVS I/O error: {e}"),
            PvsFileError::InvalidMagic(m) => {
                write!(f, "PVS invalid magic: {:?}", m)
            }
            PvsFileError::VersionTooNew {
                got,
                max,
            } => {
                write!(f, "PVS version {got} > max supported {max}")
            }
            PvsFileError::SubsectorCountMismatch {
                expected,
                got,
            } => {
                write!(
                    f,
                    "PVS subsector count mismatch: expected {expected}, got {got}"
                )
            }
            PvsFileError::TruncatedHeader => write!(f, "PVS file is truncated (header incomplete)"),
        }
    }
}

impl std::error::Error for PvsFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PvsFileError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for PvsFileError {
    fn from(e: io::Error) -> Self {
        PvsFileError::Io(e)
    }
}

// ============================================================================
// FILE HEADER
// ============================================================================

/// Parsed representation of the 24-byte PVS file header.
pub struct PvsFileHeader {
    /// Format identifier bytes (e.g. `b"PV2D"`).
    pub magic: [u8; 4],
    /// Format version number.
    pub version: u8,
    /// Number of subsectors the PVS data covers.
    pub subsector_count: usize,
    /// Number of `u32` words in the visibility data section.
    pub data_len: usize,
}

impl PvsFileHeader {
    /// Size of the serialised header in bytes.
    pub const SIZE: usize = 24;

    /// Returns `true` if the header's magic and version are acceptable.
    pub fn is_valid_for(&self, magic: [u8; 4], max_version: u8) -> bool {
        self.magic == magic && self.version <= max_version
    }
}

impl TryFrom<&[u8]> for PvsFileHeader {
    type Error = PvsFileError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() < Self::SIZE {
            return Err(PvsFileError::TruncatedHeader);
        }
        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        let version = bytes[4];
        // bytes[5..8] are reserved
        let subsector_count = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
        let data_len = u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize;
        Ok(PvsFileHeader {
            magic,
            version,
            subsector_count,
            data_len,
        })
    }
}

// ============================================================================
// TRAITS
// ============================================================================

/// Core visibility query interface implemented by all PVS backends.
pub trait PvsData {
    /// Returns `true` if subsector `to` is potentially visible from subsector
    /// `from`.
    fn is_visible(&self, from: usize, to: usize) -> bool;

    /// Returns all subsector indices potentially visible from subsector `from`.
    fn get_visible_subsectors(&self, from: usize) -> Vec<usize>;

    /// Total number of subsectors covered by this PVS.
    fn subsector_count(&self) -> usize;

    /// Count of (from, to) pairs where `is_visible` is `true`.
    fn count_visible_pairs(&self) -> u64;
}

/// Implemented by PVS types that expose a 2D portal graph for visualisation.
pub trait PvsView2D {
    /// Return a reference to the portal adjacency structure.
    fn portals_2d(&self) -> &Portals;
}

/// Coarse visibility estimation interface used during PVS construction.
///
/// Provides the per-source and per-portal-direction bitsets that bound the
/// full frustum-clip traversal.
pub trait MightSee {
    /// Total number of subsectors.
    fn subsector_count(&self) -> usize;

    /// Bitset row for the conservative set of subsectors visible from `source`.
    fn source_bits(&self, source: usize) -> &[u32];

    /// Bitset row for the conservative set of subsectors visible through portal
    /// `pi` in direction `side` (0 = from `subsector_b`, 1 = from
    /// `subsector_a`).
    fn portal_dir_bits(&self, pi: usize, side: usize) -> &[u32];
}

/// Serialisable PVS file format.
///
/// Implementors must also implement `TryFrom<&[u8], Error = PvsFileError>`
/// (the deserialization direction). Default methods handle file I/O and
/// cache path construction.
pub trait PvsFile: for<'a> TryFrom<&'a [u8], Error = PvsFileError> {
    /// Magic bytes identifying this format variant in cache files.
    const MAGIC: [u8; 4];

    /// Highest format version this build can read.
    const MAX_VERSION: u8;

    /// Serialize the PVS data to raw bytes (header followed by payload).
    fn to_bytes(&self) -> Vec<u8>;

    /// Read and deserialize a PVS from `r` by loading all bytes then calling
    /// `Self::try_from`.
    fn load_from_reader<R: Read>(r: &mut R) -> Result<Self, PvsFileError>
    where
        Self: Sized,
    {
        let mut buf = Vec::new();
        r.read_to_end(&mut buf)?;
        Self::try_from(buf.as_slice())
    }

    /// Serialize and write this PVS to the platform cache directory.
    fn save_to_cache(&self, map_name: &str, map_hash: u64) -> Result<(), PvsFileError>
    where
        Self: Sized,
    {
        let path = Self::cache_path(map_name, map_hash)?;
        let bytes = self.to_bytes();
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Compute the cache file path for the given map name and hash.
    ///
    /// Creates the cache directory if it does not already exist.
    fn cache_path(map_name: &str, map_hash: u64) -> Result<PathBuf, PvsFileError>
    where
        Self: Sized,
    {
        let cache_dir = dirs::cache_dir()
            .ok_or_else(|| {
                PvsFileError::Io(io::Error::new(io::ErrorKind::NotFound, "no cache dir"))
            })?
            .join("room4doom")
            .join("pvs");
        std::fs::create_dir_all(&cache_dir)?;
        Ok(cache_dir.join(format!("{map_name}_{map_hash}.pvs")))
    }
}

// ============================================================================
// RENDER PVS — ROW-MAJOR BITSET
// ============================================================================

/// Compact row-major bitset storing the computed PVS for every subsector.
///
/// Row `from` starts at word index `from * row_words()` and has `row_words()`
/// words. Bit `to % 32` of word `from * row_words() + to / 32` is set when
/// subsector `to` is visible from subsector `from`.
///
/// An empty `data` vec is treated as "all visible" (uncomputed / trivial map).
#[derive(Default, Clone)]
pub struct RenderPvs {
    pub(crate) subsector_count: usize,
    pub(crate) data: Vec<u32>,
}

impl RenderPvs {
    /// Number of `u32` words per row (one row per subsector).
    pub(crate) fn row_words(&self) -> usize {
        (self.subsector_count + 31) / 32
    }

    /// Return the bitset row for subsector `from`.
    pub(crate) fn row_slice(&self, from: usize) -> &[u32] {
        let w = self.row_words();
        let start = from * w;
        &self.data[start..start + w]
    }
}

impl PvsData for RenderPvs {
    fn is_visible(&self, from: usize, to: usize) -> bool {
        if self.data.is_empty() {
            return true;
        }
        let w = self.row_words();
        (self.data[from * w + to / 32] & (1u32 << (to % 32))) != 0
    }

    fn get_visible_subsectors(&self, from: usize) -> Vec<usize> {
        if self.data.is_empty() {
            return (0..self.subsector_count).collect();
        }
        let w = self.row_words();
        let start = from * w;
        let mut out = Vec::new();
        for i in 0..w {
            let mut word = self.data[start + i];
            let base = i * 32;
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                let ss = base + bit;
                if ss < self.subsector_count {
                    out.push(ss);
                }
                word &= word - 1;
            }
        }
        out
    }

    fn subsector_count(&self) -> usize {
        self.subsector_count
    }

    fn count_visible_pairs(&self) -> u64 {
        self.data.iter().map(|w| w.count_ones() as u64).sum()
    }
}

impl TryFrom<&[u8]> for RenderPvs {
    type Error = PvsFileError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let header = PvsFileHeader::try_from(bytes)?;

        if header.magic != Self::MAGIC {
            return Err(PvsFileError::InvalidMagic(header.magic));
        }
        if header.version > Self::MAX_VERSION {
            return Err(PvsFileError::VersionTooNew {
                got: header.version,
                max: Self::MAX_VERSION,
            });
        }

        let data_bytes = header.data_len * 4;
        if bytes.len() < PvsFileHeader::SIZE + data_bytes {
            return Err(PvsFileError::TruncatedHeader);
        }

        let raw = &bytes[PvsFileHeader::SIZE..PvsFileHeader::SIZE + data_bytes];
        let data: Vec<u32> = raw
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();

        Ok(RenderPvs {
            subsector_count: header.subsector_count,
            data,
        })
    }
}

impl PvsFile for RenderPvs {
    const MAGIC: [u8; 4] = *b"PV2D";
    const MAX_VERSION: u8 = 1;

    fn to_bytes(&self) -> Vec<u8> {
        let data_len = self.data.len();
        let mut out = Vec::with_capacity(PvsFileHeader::SIZE + data_len * 4);

        // Magic
        out.extend_from_slice(&Self::MAGIC);
        // Version + 3 reserved bytes
        out.push(Self::MAX_VERSION);
        out.extend_from_slice(&[0u8; 3]);
        // subsector_count as u64 LE
        out.extend_from_slice(&(self.subsector_count as u64).to_le_bytes());
        // data_len as u64 LE
        out.extend_from_slice(&(data_len as u64).to_le_bytes());
        // Visibility words
        for &word in &self.data {
            out.extend_from_slice(&word.to_le_bytes());
        }

        out
    }
}

// ============================================================================
// CACHE LOAD HELPER
// ============================================================================

/// Attempt to load a [`RenderPvs`] from the platform cache.
///
/// Returns `None` if no cache file exists, if the file is corrupt or from an
/// incompatible format version, or if the recorded subsector count does not
/// match `expected_subsectors`.
pub fn pvs_load_from_cache(
    map_name: &str,
    map_hash: u64,
    expected_subsectors: usize,
) -> Option<RenderPvs> {
    let cache_path = RenderPvs::cache_path(map_name, map_hash).ok()?;
    if !cache_path.exists() {
        return None;
    }
    log::info!("Found PVS cache at {cache_path:?}");
    let mut f = std::fs::File::open(&cache_path).ok()?;
    let pvs = RenderPvs::load_from_reader(&mut f).ok()?;
    if pvs.subsector_count() != expected_subsectors {
        log::warn!(
            "PVS cache subsector count mismatch: expected {expected_subsectors}, got {}; ignoring cache",
            pvs.subsector_count()
        );
        return None;
    }
    Some(pvs)
}
