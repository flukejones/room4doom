//! Typed linedef + thing flag bits, mirroring the game engine's
//! `level::LineDefFlags` (vanilla plus BOOM's `PassUse`).
//!
//! `flags`/`options` are imported and exported verbatim, so the bit values must
//! match the WAD. Serde (de)serializes each set as its raw `i32` — the same
//! representation the fields had as plain integers — so undo snapshots and RON
//! maps written before this type are byte-compatible. Bits outside the named set
//! (port-specific extensions) are retained, not dropped, so they round-trip.

use bitflags::bitflags;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

bitflags! {
    /// Linedef flags (classic Doom `ML_*`). The editor interprets only
    /// [`LineFlags::TWO_SIDED`]; the rest round-trip through import/export.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct LineFlags: i32 {
        const BLOCKING = 1;
        const BLOCK_MONSTERS = 1 << 1;
        const TWO_SIDED = 1 << 2;
        const UNPEG_TOP = 1 << 3;
        const UNPEG_BOTTOM = 1 << 4;
        const SECRET = 1 << 5;
        const BLOCK_SOUND = 1 << 6;
        const UNMAPPED = 1 << 7;
        const MAPPED = 1 << 8;
        const PASS_USE = 1 << 9;
    }
}

bitflags! {
    /// Thing option flags (vanilla `MTF_*`): difficulty presence, deaf/ambush,
    /// and multiplayer-only. Drives the editor's skill filter and thing panel.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct ThingFlags: i32 {
        const EASY = 1;
        const NORMAL = 1 << 1;
        const HARD = 1 << 2;
        const AMBUSH = 1 << 3;
        const MULTIPLAYER = 1 << 4;
    }
}

impl Serialize for LineFlags {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.bits().serialize(s)
    }
}

impl<'de> Deserialize<'de> for LineFlags {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::from_bits_retain(i32::deserialize(d)?))
    }
}

impl Serialize for ThingFlags {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.bits().serialize(s)
    }
}

impl<'de> Deserialize<'de> for ThingFlags {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::from_bits_retain(i32::deserialize(d)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ron::{from_str, to_string};

    #[test]
    fn line_flags_serialize_as_bare_int() {
        // The field used to be a plain i32, so RON/undo must still see one int.
        let f = LineFlags::TWO_SIDED | LineFlags::UNPEG_TOP;
        assert_eq!(to_string(&f).expect("ser"), "12");
        assert_eq!(from_str::<LineFlags>("12").expect("de"), f);
    }

    #[test]
    fn unknown_bits_round_trip() {
        // A WAD line with bits the editor does not name must survive import/export.
        let f = LineFlags::from_bits_retain(0x7fff);
        assert_eq!(
            from_str::<LineFlags>(&to_string(&f).expect("ser")).expect("de"),
            f
        );
    }

    #[test]
    fn thing_flags_bits_match_mtf() {
        assert_eq!(ThingFlags::EASY.bits(), 1);
        assert_eq!(ThingFlags::HARD.bits(), 4);
        assert_eq!(ThingFlags::MULTIPLAYER.bits(), 16);
    }
}
