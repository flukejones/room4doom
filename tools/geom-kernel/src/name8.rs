//! 8-byte, NUL-padded texture/flat/patch names with DoomEd `char[9]` semantics.
//!
//! An all-NUL value means "no texture". The .dwd text format serializes the
//! empty name as `"-"`; the wad crate parses the WAD's `"-"` to an empty
//! string. Equality and hashing operate on the zero-padded byte array, which
//! matches DoomEd's `bcmp`-based sectordef comparison exactly.

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserializer, Serializer};

/// Maximum bytes in a Doom texture/flat/patch name.
pub const NAME_LEN: usize = 8;
/// Placeholder used by the .dwd text format for an empty name.
pub const DWD_EMPTY_NAME: &str = "-";

/// An 8-byte, NUL-padded name, canonicalized to uppercase on construction.
/// Doom texture/flat/patch names are case-insensitive, so equality and hashing
/// over the byte array are case-insensitive matches with no per-call folding.
///
/// Serializes as the plain uppercase string (empty name → `""`) so RON/JSON
/// stay human-readable, not the raw byte array.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Name8([u8; NAME_LEN]);

/// Why a string cannot become a [`Name8`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    /// More than [`NAME_LEN`] bytes.
    TooLong(String),
    /// Contains a byte outside printable ASCII (controls, NUL, or non-ASCII).
    BadChar(String),
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong(name) => write!(f, "name longer than {NAME_LEN} bytes: {name:?}"),
            Self::BadChar(name) => write!(f, "name contains invalid characters: {name:?}"),
        }
    }
}

impl std::error::Error for NameError {}

impl Name8 {
    /// The "no texture" value.
    pub const EMPTY: Self = Self([0; NAME_LEN]);

    /// Build from a string of 1..=8 printable, non-whitespace ASCII bytes,
    /// uppercased (Doom names are case-insensitive). Empty input → [`Name8::EMPTY`].
    pub fn new(name: &str) -> Result<Self, NameError> {
        let bytes = name.as_bytes();
        if bytes.len() > NAME_LEN {
            return Err(NameError::TooLong(name.to_owned()));
        }
        if bytes.iter().any(|&b| !b.is_ascii_graphic()) {
            return Err(NameError::BadChar(name.to_owned()));
        }
        let mut buf = [0u8; NAME_LEN];
        buf[..bytes.len()].copy_from_slice(bytes);
        buf.make_ascii_uppercase();
        Ok(Self(buf))
    }

    /// True for the "no texture" value.
    pub fn is_empty(&self) -> bool {
        self.0[0] == 0
    }

    /// The name without NUL padding.
    pub fn as_str(&self) -> &str {
        let end = self.0.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
        str::from_utf8(&self.0[..end]).expect("Name8 bytes are constructor-validated ASCII")
    }

    /// Parse a .dwd field: `"-"` means empty.
    pub fn from_dwd_field(field: &str) -> Result<Self, NameError> {
        if field == DWD_EMPTY_NAME {
            Ok(Self::EMPTY)
        } else {
            Self::new(field)
        }
    }

    /// Convert a wad-crate name: the wad crate already maps `"-"` to `""`.
    pub fn from_wad(name: &str) -> Result<Self, NameError> {
        Self::new(name)
    }

    /// Serialize for .dwd output: empty becomes `"-"`.
    pub fn to_dwd_field(&self) -> &str {
        if self.is_empty() {
            DWD_EMPTY_NAME
        } else {
            self.as_str()
        }
    }

    /// Raw 8-byte WAD lump field; the empty name encodes as `"-"`.
    pub fn to_wad_bytes(&self) -> [u8; NAME_LEN] {
        if self.is_empty() {
            let mut bytes = [0u8; NAME_LEN];
            bytes[0] = b'-';
            bytes
        } else {
            self.0
        }
    }
}

impl fmt::Display for Name8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Name8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Name8({:?})", self.as_str())
    }
}

impl TryFrom<&str> for Name8 {
    type Error = NameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl serde::Serialize for Name8 {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for Name8 {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct Name8Visitor;
        impl Visitor<'_> for Name8Visitor {
            type Value = Name8;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a Doom name of up to 8 printable ASCII bytes")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Name8, E> {
                Name8::new(v).map_err(|e| E::custom(e.to_string()))
            }
        }
        de.deserialize_str(Name8Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names_round_trip() {
        let n = Name8::new("FLOOR4_8").expect("8-char name is valid");
        assert_eq!(n.as_str(), "FLOOR4_8");
        assert_eq!(n.to_string(), "FLOOR4_8");
        assert!(!n.is_empty());
    }

    #[test]
    fn empty_string_is_empty_name() {
        let n = Name8::new("").expect("empty input is the empty name");
        assert_eq!(n, Name8::EMPTY);
        assert!(n.is_empty());
        assert_eq!(n.as_str(), "");
    }

    #[test]
    fn serde_uses_plain_string_not_byte_tuple() {
        let n = Name8::new("STARTAN3").expect("valid");
        let text = ron::to_string(&n).expect("serializes");
        assert_eq!(text, "\"STARTAN3\"", "Name8 serializes as a quoted string");
        assert_eq!(ron::from_str::<Name8>(&text).expect("round-trips"), n);

        let empty = ron::to_string(&Name8::EMPTY).expect("serializes");
        assert_eq!(empty, "\"\"");
        assert_eq!(
            ron::from_str::<Name8>(&empty).expect("round-trips"),
            Name8::EMPTY
        );
    }

    #[test]
    fn nine_chars_rejected() {
        assert_eq!(
            Name8::new("ABCDEFGHI"),
            Err(NameError::TooLong("ABCDEFGHI".to_owned()))
        );
    }

    #[test]
    fn non_ascii_and_whitespace_rejected() {
        assert!(matches!(Name8::new("DÖOR"), Err(NameError::BadChar(_))));
        assert!(matches!(Name8::new("A B"), Err(NameError::BadChar(_))));
        assert!(matches!(Name8::new("A\0B"), Err(NameError::BadChar(_))));
    }

    #[test]
    fn dwd_dash_means_empty_both_directions() {
        let n = Name8::from_dwd_field("-").expect("dash is the empty name");
        assert!(n.is_empty());
        assert_eq!(n.to_dwd_field(), "-");
        let real = Name8::from_dwd_field("DOOR3").expect("plain name");
        assert_eq!(real.to_dwd_field(), "DOOR3");
    }

    #[test]
    fn eq_and_hash_use_zero_padding() {
        use std::collections::HashMap;
        let a = Name8::new("STARG3").expect("valid");
        let b = Name8::new("STARG3").expect("valid");
        assert_eq!(a, b);
        let mut m = HashMap::new();
        m.insert(a, 1);
        assert_eq!(m.get(&b), Some(&1));
        assert_ne!(
            Name8::new("STARG3").expect("valid"),
            Name8::new("STARG33").expect("valid")
        );
    }
}
