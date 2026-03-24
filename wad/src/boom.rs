/// BOOM binary lump parsers: SWITCHES, ANIMATED

const SWITCH_ENTRY_SIZE: usize = 20;
const ANIMATED_ENTRY_SIZE: usize = 23;

fn parse_name(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    std::str::from_utf8(&data[..end])
        .unwrap_or("")
        .to_ascii_uppercase()
}

#[derive(Debug, Clone)]
pub struct SwitchEntry {
    pub name1: String,
    pub name2: String,
    pub episode: i16,
}

/// Parse a binary SWITCHES lump.
/// 20 bytes per entry: 9-byte name1 + 9-byte name2 + 2-byte LE episode.
/// Terminated by episode == 0.
pub fn parse_switches(data: &[u8]) -> Vec<SwitchEntry> {
    let mut entries = Vec::new();
    let mut offset = 0;
    while offset + SWITCH_ENTRY_SIZE <= data.len() {
        let episode = i16::from_le_bytes([data[offset + 18], data[offset + 19]]);
        if episode == 0 {
            break;
        }
        let name1 = parse_name(&data[offset..offset + 9]);
        let name2 = parse_name(&data[offset + 9..offset + 18]);
        if !name1.is_empty() && !name2.is_empty() {
            entries.push(SwitchEntry {
                name1,
                name2,
                episode,
            });
        }
        offset += SWITCH_ENTRY_SIZE;
    }
    entries
}

#[derive(Debug, Clone)]
pub struct AnimatedEntry {
    pub is_texture: bool,
    pub end_name: String,
    pub start_name: String,
    pub speed: u32,
}

/// Parse a binary ANIMATED lump.
/// 23 bytes per entry: 1-byte type + 9-byte end name + 9-byte start name +
/// 4-byte LE speed. Terminated by type == 0xFF.
pub fn parse_animated(data: &[u8]) -> Vec<AnimatedEntry> {
    let mut entries = Vec::new();
    let mut offset = 0;
    while offset + ANIMATED_ENTRY_SIZE <= data.len() {
        let typ = data[offset];
        if typ == 0xFF {
            break;
        }
        let end_name = parse_name(&data[offset + 1..offset + 10]);
        let start_name = parse_name(&data[offset + 10..offset + 19]);
        let speed = u32::from_le_bytes([
            data[offset + 19],
            data[offset + 20],
            data[offset + 21],
            data[offset + 22],
        ]);
        if !end_name.is_empty() && !start_name.is_empty() {
            entries.push(AnimatedEntry {
                is_texture: typ & 1 != 0,
                end_name,
                start_name,
                speed,
            });
        }
        offset += ANIMATED_ENTRY_SIZE;
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_switches_lump() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/boom/SWITCHES.lmp"
        ))
        .expect("SWITCHES.lmp test file");
        let entries = parse_switches(&data);
        assert_eq!(entries.len(), 46);
        assert_eq!(entries[0].name1, "SW1BRCOM");
        assert_eq!(entries[0].name2, "SW2BRCOM");
        assert_eq!(entries[0].episode, 1);
        // Last valid entry before terminator
        let last = entries.last().unwrap();
        assert!(last.episode > 0);
    }

    #[test]
    fn test_parse_animated_lump() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/boom/ANIMATED.lmp"
        ))
        .expect("ANIMATED.lmp test file");
        let entries = parse_animated(&data);
        assert_eq!(entries.len(), 84);
        assert_eq!(entries[0].end_name, "NUKAGE3");
        assert_eq!(entries[0].start_name, "NUKAGE1");
        assert!(!entries[0].is_texture);
        assert_eq!(entries[0].speed, 8);
    }

    #[test]
    fn test_parse_sunder_switches() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/boom/sunder_switches.lmp"
        ))
        .expect("sunder_switches.lmp");
        let entries = parse_switches(&data);
        assert_eq!(entries.len(), 46);
        // Sunder has custom switches beyond vanilla
        let custom: Vec<_> = entries
            .iter()
            .filter(|e| e.name1.starts_with("OSWTCH"))
            .collect();
        assert!(!custom.is_empty());
    }

    #[test]
    fn test_parse_sunder_animated() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/boom/sunder_animated.lmp"
        ))
        .expect("sunder_animated.lmp");
        let entries = parse_animated(&data);
        assert!(
            entries.len() > 22,
            "Sunder should have more animations than vanilla"
        );
        // Verify mix of flats and textures
        let flats = entries.iter().filter(|e| !e.is_texture).count();
        let textures = entries.iter().filter(|e| e.is_texture).count();
        assert!(flats > 0);
        assert!(textures > 0);
    }

    #[test]
    fn test_parse_eviternity_switches() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/boom/eviternity_switches.lmp"
        ))
        .expect("eviternity_switches.lmp");
        let entries = parse_switches(&data);
        assert!(
            entries.len() > 40,
            "Eviternity should extend vanilla switches"
        );
    }

    #[test]
    fn test_parse_eviternity_animated() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/boom/eviternity_animated.lmp"
        ))
        .expect("eviternity_animated.lmp");
        let entries = parse_animated(&data);
        assert!(
            entries.len() > 22,
            "Eviternity should extend vanilla animations"
        );
    }

    #[test]
    fn test_parse_sigil2_switches() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/boom/sigil2_switches.lmp"
        ))
        .expect("sigil2_switches.lmp");
        let entries = parse_switches(&data);
        assert!(!entries.is_empty());
    }

    #[test]
    fn test_parse_sigil2_animated() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/boom/sigil2_animated.lmp"
        ))
        .expect("sigil2_animated.lmp");
        let entries = parse_animated(&data);
        assert!(!entries.is_empty());
    }
}
