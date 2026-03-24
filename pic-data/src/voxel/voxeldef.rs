/// VOXELDEF.txt parser — maps sprite frame names to KVX files.

pub struct VoxelDef {
    pub sprite_name: String,
    pub kvx_file: String,
    pub placed_spin: Option<i32>,
    pub dropped_spin: Option<i32>,
    pub angle_offset: Option<i32>,
}

pub fn parse(text: &str) -> Vec<VoxelDef> {
    let mut defs = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if let Some(def) = parse_line(line) {
            defs.push(def);
        }
    }
    defs
}

fn parse_line(line: &str) -> Option<VoxelDef> {
    // Format: sprite_name = "kvx_filename" { key = value ... }
    let eq_pos = line.find('=')?;
    let sprite_name = line[..eq_pos].trim().to_string();

    let rest = line[eq_pos + 1..].trim();
    // Extract quoted filename
    let q1 = rest.find('"')?;
    let q2 = rest[q1 + 1..].find('"')? + q1 + 1;
    let kvx_file = rest[q1 + 1..q2].to_string();

    let mut placed_spin = None;
    let mut dropped_spin = None;
    let mut angle_offset = None;

    // Parse brace block
    if let Some(brace_start) = rest.find('{') {
        if let Some(brace_end) = rest.find('}') {
            let block = &rest[brace_start + 1..brace_end];
            for pair in block.split_whitespace().collect::<Vec<_>>().chunks(3) {
                // key = value
                if pair.len() >= 3 && pair[1] == "=" {
                    let key = pair[0];
                    let val: i32 = match pair[2].parse() {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    match key {
                        "PlacedSpin" => placed_spin = Some(val),
                        "DroppedSpin" => dropped_spin = Some(val),
                        "AngleOffset" => angle_offset = Some(val),
                        _ => {}
                    }
                }
            }
        }
    }

    Some(VoxelDef {
        sprite_name,
        kvx_file,
        placed_spin,
        dropped_spin,
        angle_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_voxeldef() {
        let input = r#"
// Chainsaw
csawa = "csawa" { PlacedSpin = 150 DroppedSpin = 150 }

// Pistol
pista = "pista" { PlacedSpin = 150 DroppedSpin = 150 }

// Bullet (clip)
clipa = "clipa" { AngleOffset = 270 }

// Shells
shela = "shela" {}

// Rocket explosion
mislb = "mislb" { AngleOffset = -90 }
"#;
        let defs = parse(input);
        assert_eq!(defs.len(), 5);

        assert_eq!(defs[0].sprite_name, "csawa");
        assert_eq!(defs[0].kvx_file, "csawa");
        assert_eq!(defs[0].placed_spin, Some(150));
        assert_eq!(defs[0].dropped_spin, Some(150));
        assert_eq!(defs[0].angle_offset, None);

        assert_eq!(defs[2].sprite_name, "clipa");
        assert_eq!(defs[2].angle_offset, Some(270));
        assert_eq!(defs[2].placed_spin, None);

        assert_eq!(defs[3].sprite_name, "shela");
        assert_eq!(defs[3].placed_spin, None);
        assert_eq!(defs[3].dropped_spin, None);

        assert_eq!(defs[4].sprite_name, "mislb");
        assert_eq!(defs[4].angle_offset, Some(-90));
    }
}
