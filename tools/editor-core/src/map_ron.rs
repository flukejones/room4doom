//! Native map file I/O: `EditorMap` serialised as RON.
//!
//! The DoomEd `.dwd` text format is read-only (import); the editor's own
//! working format is RON, a direct serde round-trip of [`EditorMap`] with no
//! hand-written parser. Geometry only — imported patch lumps and the project
//! manifest live elsewhere (see [`crate::project`]).

use std::fmt;
use std::io;
use std::path::Path;

use ron::ser::PrettyConfig;

use crate::model::EditorMap;

/// File extension for native map files.
pub const MAP_RON_EXT: &str = "ron";

/// Failure while reading or writing a native `.ron` map.
#[derive(Debug)]
pub enum MapRonError {
    Io(io::Error),
    Serialize(ron::Error),
    Deserialize(ron::error::SpannedError),
}

impl fmt::Display for MapRonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "map io error: {e}"),
            Self::Serialize(e) => write!(f, "map serialize error: {e}"),
            Self::Deserialize(e) => write!(f, "map parse error: {e}"),
        }
    }
}

impl std::error::Error for MapRonError {}

impl From<io::Error> for MapRonError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Serialise a map to RON text (pretty-printed for legible diffs).
pub fn write_map_ron(map: &EditorMap) -> Result<String, MapRonError> {
    ron::ser::to_string_pretty(map, PrettyConfig::default()).map_err(MapRonError::Serialize)
}

/// Parse a map from RON text.
pub fn parse_map_ron(text: &str) -> Result<EditorMap, MapRonError> {
    ron::from_str(text).map_err(MapRonError::Deserialize)
}

/// Read and parse a native `.ron` map file.
pub fn load_map_ron(path: &Path) -> Result<EditorMap, MapRonError> {
    let text = std::fs::read_to_string(path)?;
    parse_map_ron(&text)
}

/// Serialise and write a native `.ron` map file.
pub fn save_map_ron(path: &Path, map: &EditorMap) -> Result<(), MapRonError> {
    let text = write_map_ron(map)?;
    std::fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dwd::parse_dwd;

    const FIXTURE: &str = include_str!("../../doomed-parser/tests/fixtures/E1M1.dwd");

    #[test]
    fn ron_round_trips_an_imported_map() {
        let mut map = parse_dwd(FIXTURE).expect("fixture parses");
        map.required_wads = vec!["doom2.wad".to_owned(), "sunder.wad".to_owned()];
        let text = write_map_ron(&map).expect("serialises");
        let back = parse_map_ron(&text).expect("parses");
        assert_eq!(map, back);
        assert_eq!(back.required_wads, ["doom2.wad", "sunder.wad"]);
    }

    #[test]
    fn old_map_without_required_wads_loads() {
        let map = parse_dwd(FIXTURE).expect("fixture parses");
        let text = write_map_ron(&map).expect("serialises");
        let stripped: String = text
            .lines()
            .filter(|l| !l.contains("required_wads"))
            .collect::<Vec<_>>()
            .join("\n");
        let back = parse_map_ron(&stripped).expect("old map still parses");
        assert!(back.required_wads.is_empty());
    }
}
