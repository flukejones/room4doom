use super::{BossActions, MapEntry, UMapInfo, parse_map_name};
use std::collections::HashMap;

pub fn parse_mapinfo(input: &str) -> Result<UMapInfo, String> {
    let mut entries = Vec::new();
    let mut index = HashMap::new();
    let mut current: Option<MapEntry> = None;

    for raw_line in input.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        let lower = line.to_ascii_lowercase();

        // Skip global directives
        if lower.starts_with("defaultmap")
            || lower.starts_with("nocrouch")
            || lower.starts_with("nojump")
            || lower.starts_with("nofreelook")
            || lower.starts_with("adddefaultmap")
            || lower.starts_with("gamedefaults")
        {
            continue;
        }

        // New map block
        if lower.starts_with("map ") {
            if let Some(entry) = current.take() {
                let key = entry.map_name.clone();
                let idx = entries.len();
                entries.push(entry);
                index.insert(key, idx);
            }
            current = Some(parse_map_header(line));
            continue;
        }

        // Cluster/episode definition at top level (outside a map block)
        if current.is_none() && (lower.starts_with("cluster ") || lower.starts_with("episode ")) {
            continue;
        }

        // Key-value within a map block
        let Some(ref mut entry) = current else {
            continue;
        };

        let (key, value) = split_key_value(line);
        match key.as_str() {
            "next" => entry.next = Some(value.to_ascii_uppercase()),
            "secretnext" => entry.next_secret = Some(value.to_ascii_uppercase()),
            "sky1" => {
                let tex = value.split_whitespace().next().unwrap_or("");
                if !tex.is_empty() {
                    entry.sky_texture = Some(tex.to_ascii_uppercase());
                }
            }
            "music" => entry.music = Some(value.to_ascii_uppercase()),
            "author" => entry.author = Some(unquote(&value)),
            "levelname" => entry.level_name = Some(unquote(&value)),
            "titlepatch" => entry.level_pic = Some(value.to_ascii_uppercase()),
            "levelpic" => entry.level_pic = Some(value.to_ascii_uppercase()),
            "exitpic" => entry.exit_pic = Some(unquote(&value)),
            "enterpic" => entry.enter_pic = Some(unquote(&value)),
            "partime" => entry.par_time = value.parse().ok(),
            "cluster" => {} // cluster assignment — tracked but not used yet
            "map07special" => {
                entry.boss_actions = Some(BossActions::Actions(vec![
                    super::BossAction {
                        thing_type: "Fatso".to_string(),
                        line_special: 23,
                        tag: 666,
                    },
                    super::BossAction {
                        thing_type: "Arachnotron".to_string(),
                        line_special: 30,
                        tag: 667,
                    },
                ]));
            }
            "baronspecial" => {
                entry.boss_actions = Some(BossActions::Actions(vec![super::BossAction {
                    thing_type: "BaronOfHell".to_string(),
                    line_special: 23,
                    tag: 666,
                }]));
            }
            "cyberdemonspecial" => {
                entry.boss_actions = Some(BossActions::Actions(vec![super::BossAction {
                    thing_type: "Cyberdemon".to_string(),
                    line_special: 23,
                    tag: 666,
                }]));
            }
            "spidermastermindspecial" => {
                entry.boss_actions = Some(BossActions::Actions(vec![super::BossAction {
                    thing_type: "SpiderMastermind".to_string(),
                    line_special: 23,
                    tag: 666,
                }]));
            }
            "nointermission" => entry.no_intermission = true,
            "endgame" => entry.end_game = Some(true),
            "endbunny" => entry.end_bunny = true,
            "endcast" => entry.end_cast = true,
            _ => {
                // Unknown key — skip silently (MAPINFO has many keys we don't
                // support)
            }
        }
    }

    // Flush last entry
    if let Some(entry) = current.take() {
        let key = entry.map_name.clone();
        let idx = entries.len();
        entries.push(entry);
        index.insert(key, idx);
    }

    Ok(UMapInfo {
        entries,
        index,
        clear_episodes: false,
    })
}

fn parse_map_header(line: &str) -> MapEntry {
    // "map MAP01 "Python"" or "map MAP01 lookup YOURNAME"
    let rest = line[4..].trim();
    let (map_name, remainder) = split_first_token(rest);
    let map_name = map_name.to_ascii_uppercase();
    let (episode, map) = parse_map_name(&map_name);

    let level_name = if !remainder.is_empty() {
        let name = unquote(remainder.trim());
        if name.to_ascii_lowercase().starts_with("lookup") {
            None // "lookup" references — not supported
        } else if name.is_empty() {
            None
        } else {
            Some(name)
        }
    } else {
        None
    };

    MapEntry {
        map_name,
        episode,
        map,
        level_name,
        ..Default::default()
    }
}

fn strip_comment(line: &str) -> &str {
    if let Some(pos) = line.find("//") {
        &line[..pos]
    } else {
        line
    }
}

fn split_key_value(line: &str) -> (String, String) {
    // Handle both "key = value" and "key value" syntax
    if let Some(eq_pos) = line.find('=') {
        let key = line[..eq_pos].trim().to_ascii_lowercase();
        let val = line[eq_pos + 1..].trim().to_string();
        (key, val)
    } else {
        let (key, val) = split_first_token(line);
        (key.to_ascii_lowercase(), val.to_string())
    }
}

fn split_first_token(s: &str) -> (String, &str) {
    let s = s.trim();
    if s.starts_with('"') {
        // Quoted token
        if let Some(end) = s[1..].find('"') {
            let token = s[1..1 + end].to_string();
            let rest = s[2 + end..].trim();
            return (token, rest);
        }
    }
    match s.find(|c: char| c.is_whitespace()) {
        Some(pos) => (s[..pos].to_string(), s[pos..].trim()),
        None => (s.to_string(), ""),
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUNDER_MAPINFO: &str = r#"
//************************************//
//Sunder MAPINFO                      //
//************************************//

defaultmap
nocrouch
nojump

map MAP01 "Python"
titlepatch CWILV00
next MAP02
secretnext MAP02
sky1 RSKY1 0
cluster 5
music D_RUNNIN

map MAP02 "The Burrow"
titlepatch CWILV01
next MAP03
secretnext MAP03
sky1 RSKY1 0
cluster 5
music D_STALKS

map MAP07 "Hollow Icon"
titlepatch CWILV06
next MAP08
secretnext MAP08
sky1 RSKY1 0
cluster 6
map07special
music D_SHAWN

map MAP15 "Babylon's Chimera"
titlepatch CWILV14
next MAP16
secretnext MAP31
sky1 RSKY2 0
cluster 6
music D_RUNNI2

map MAP18 "The Singing Void"
titlepatch CWILV17
next MAP19
secretnext MAP19
sky1 SPACESK2 0
cluster 2
music D_ROMERO

map MAP31 "House of Corrosion"
titlepatch CWILV31
next MAP16
secretnext MAP32
sky1 GRSKY1 0
cluster 6
music D_EVIL

map MAP32 "The Harlot's Garden"
titlepatch CWILV32
next MAP16
secretnext MAP32
sky1 RSKY3 0
cluster 6
music D_ULTIMA
"#;

    #[test]
    fn test_sunder_mapinfo() {
        let info = parse_mapinfo(SUNDER_MAPINFO).expect("parse failed");
        assert_eq!(info.entries().len(), 7);

        let m01 = info.get("MAP01").expect("MAP01 missing");
        assert_eq!(m01.level_name.as_deref(), Some("Python"));
        assert_eq!(m01.level_pic.as_deref(), Some("CWILV00"));
        assert_eq!(m01.next.as_deref(), Some("MAP02"));
        assert_eq!(m01.next_secret.as_deref(), Some("MAP02"));
        assert_eq!(m01.sky_texture.as_deref(), Some("RSKY1"));
        assert_eq!(m01.music.as_deref(), Some("D_RUNNIN"));
        assert_eq!(m01.episode, 0);
        assert_eq!(m01.map, 1);

        let m07 = info.get("MAP07").expect("MAP07 missing");
        assert_eq!(m07.level_name.as_deref(), Some("Hollow Icon"));
        assert!(matches!(m07.boss_actions, Some(BossActions::Actions(ref a)) if a.len() == 2));

        let m15 = info.get("MAP15").expect("MAP15 missing");
        assert_eq!(m15.next.as_deref(), Some("MAP16"));
        assert_eq!(m15.next_secret.as_deref(), Some("MAP31"));

        let m18 = info.get("MAP18").expect("MAP18 missing");
        assert_eq!(m18.sky_texture.as_deref(), Some("SPACESK2"));

        let m31 = info.get("MAP31").expect("MAP31 missing");
        assert_eq!(m31.next.as_deref(), Some("MAP16"));
        assert_eq!(m31.next_secret.as_deref(), Some("MAP32"));
        assert_eq!(m31.sky_texture.as_deref(), Some("GRSKY1"));
        assert_eq!(m31.music.as_deref(), Some("D_EVIL"));
    }

    #[test]
    fn test_empty_mapinfo() {
        let info = parse_mapinfo("").expect("parse failed");
        assert_eq!(info.entries().len(), 0);
    }

    #[test]
    fn test_comments_only() {
        let info = parse_mapinfo("// just comments\n// more\n").expect("parse failed");
        assert_eq!(info.entries().len(), 0);
    }

    #[test]
    fn test_defaultmap_skip() {
        let input = "defaultmap\nnocrouch\nnojump\n\nmap MAP01 \"Test\"\nnext MAP02\n";
        let info = parse_mapinfo(input).expect("parse failed");
        assert_eq!(info.entries().len(), 1);
        assert_eq!(info.get("MAP01").unwrap().next.as_deref(), Some("MAP02"));
    }

    #[test]
    fn test_sunder_mapinfo_file() {
        let data = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/mapinfo/sunder_mapinfo.txt"
        ))
        .expect("sunder_mapinfo.txt");
        let info = parse_mapinfo(&data).expect("parse failed");
        // Sunder has 22 maps
        assert!(info.entries().len() >= 20);
        let m01 = info.get("MAP01").expect("MAP01 missing");
        assert_eq!(m01.level_name.as_deref(), Some("Python"));
        assert_eq!(m01.sky_texture.as_deref(), Some("RSKY1"));
        let m20 = info.get("MAP20").expect("MAP20 missing");
        assert!(m20.level_name.is_some());
    }
}
