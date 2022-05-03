mod info_strings;

use crate::info_strings::{
    MOBJ_INFO_ARRAY_END_STR, MOBJ_INFO_ARRAY_STR, MOBJ_INFO_HEADER_STR, MOBJ_INFO_TYPE_STR,
};
use gumdrop::Options;
use std::collections::HashMap;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;

// pub struct State {
//     /// Sprite to use
//     pub sprite: SpriteNum,
//     /// The frame within this sprite to show for the state
//     pub frame: u32,
//     /// How many tics this state takes. On nightmare it is shifted >> 1
//     pub tics: i32,
//     // void (*action) (): i32,
//     /// An action callback to run on this state
//     pub action: ActionF,
//     /// The state that should come after this. Can be looped.
//     pub next_state: StateNum,
//     /// Don't know, Doom seems to set all to zero
//     pub misc1: i32,
//     /// Don't know, Doom seems to set all to zero
//     pub misc2: i32,
// }

#[derive(PartialOrd, PartialEq)]
enum LineState {
    StateType,
    InfoType(String),
    None,
}

#[derive(Debug, Clone, Options)]
struct CLIOptions {
    #[options(no_short, meta = "", help = "path to info data")]
    info: PathBuf,
    #[options(no_short, meta = "", help = "path to write generated files to")]
    out: PathBuf,
    #[options(help = "game options help")]
    help: bool,
}

type InfoType = HashMap<String, String>;
type InfoGroupType = HashMap<String, InfoType>;

fn main() -> Result<(), Box<dyn Error>> {
    let options = CLIOptions::parse_args_default_or_exit();
    let data = read_file(options.info);

    // Lines starting with:
    // - `;` are comments
    // - `$` are MapObjInfo, and may not include all possible fields
    // - `S_` are `StateNum::S_*`, and `State`
    //
    // An `S_` is unique and should accumulate in order
    // `S_` line order: statename  sprite  frame tics action nextstate [optional1] [optional2]
    //
    // SfxEnum are pre-determined?

    let (order, info) = parse_info(&data);
    write_info_file(&order, info, options.out);
    Ok(())
}

fn read_file(path: PathBuf) -> String {
    let mut file = OpenOptions::new()
        .read(true)
        .open(path.clone())
        .unwrap_or_else(|e| panic!("Couldn't open {:?}, {}", path, e));

    let mut buf = String::new();
    if file
        .read_to_string(&mut buf)
        .unwrap_or_else(|e| panic!("Couldn't read {:?}, {}", path, e))
        == 0
    {
        panic!("File had no data");
    }
    buf
}

fn write_info_file(ordering: &[String], info: InfoGroupType, path: PathBuf) {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path.clone())
        .unwrap_or_else(|e| panic!("Couldn't open {:?}, {}", path, e));

    file.write_all(MOBJ_INFO_HEADER_STR.as_bytes()).unwrap();
    file.write_all(MOBJ_INFO_TYPE_STR.as_bytes()).unwrap();
    file.write_all(MOBJ_INFO_ARRAY_STR.as_bytes()).unwrap();
    for key in ordering.iter() {
        let value = info.get(key).unwrap();
        let info = info_to_string(key, value);
        file.write_all(info.as_bytes()).unwrap();
    }
    file.write_all(MOBJ_INFO_ARRAY_END_STR.as_bytes()).unwrap();
}

fn parse_info(input: &str) -> (Vec<String>, InfoGroupType) {
    // K/V = key/mobj name, <K= field, (data, comment)>
    let mut info: InfoGroupType = HashMap::new();

    let mut ordering = Vec::new();
    let mut misc_count = 0;
    let mut line_state = LineState::None;
    for line in input.lines() {
        if line.starts_with("S_") {
            line_state = LineState::StateType;
        }
        if line.starts_with('$') {
            let split: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
            if split[1].contains("DEFAULT") {
                // ignore this one
                continue;
            }
            if split.len() == 2 {
                // A full def
                line_state = LineState::InfoType(split[1].clone());
                info.insert(split[1].clone(), HashMap::new());
                ordering.push(split[1].clone());
            } else {
                // Or one of:
                if split[1] == "+" {
                    // A misc object:
                    // $ + doomednum 2023 spawnstate S_PSTR 	flags 	MF_SPECIAL|MF_COUNTITEM
                    info.insert(format!("MT_MISC{misc_count}"), HashMap::new());
                    ordering.push(format!("MT_MISC{misc_count}"));
                    misc_count += 1;
                } else {
                    // Must be a single line misc:
                    // $ MT_INV doomednum 2022 spawnstate S_PINV 	flags 	MF_SPECIAL|MF_COUNTITEM
                    info.insert(split[1].clone(), HashMap::new());
                    ordering.push(split[1].clone());
                }
            }
            continue;
        }

        if let LineState::InfoType(name) = &mut line_state {
            if line.is_empty() || line.starts_with(' ') {
                // reset
                line_state = LineState::None;
                continue;
            }
            let split: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
            if split.len() < 2 {
                continue;
            } else if let Some(entry) = info.get_mut(name) {
                entry.insert(split[0].clone(), validate_field(&split[1]));
            }
        }
    }

    (ordering, info)
}

fn validate_field(input: &str) -> String {
    if input.contains("*FRACUNIT") {
        // Convert to something we can parse with f32
        let mut tmp = input.trim_end_matches("*FRACUNIT").to_string();
        tmp.push_str(".0");
        tmp
    } else if input.starts_with("S_") {
        // Stat number
        let mut tmp = "StateNum::".to_string();
        tmp.push_str(input);
        tmp
    } else if input.starts_with("sfx_") {
        // Sound
        let mut tmp = "SfxEnum::".to_string();
        tmp.push_str(capitalize(input.trim_start_matches("sfx_")).as_str());
        tmp
    } else if input.starts_with("MF_") {
        // Flag
        let mut tmp = String::new();
        if input.split('|').count() == 0 {
            let append = input.trim_start_matches("MF_").to_ascii_lowercase();
            tmp.push_str(format!("MapObjFlag::{} as u32", capitalize(&append)).as_str());
        } else {
            for mf in input.split('|') {
                let append = mf.trim_start_matches("MF_").to_ascii_lowercase();
                tmp.push_str(format!("MapObjFlag::{} as u32 |", capitalize(&append)).as_str());
            }
            tmp = tmp.trim_end_matches('|').to_string();
        }
        tmp
    } else {
        input.to_string()
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

fn info_to_string(name: &str, info: &InfoType) -> String {
    format!(
        r#"
// {name}
MapObjInfo {{
doomednum: {doomednum},
spawnstate: {spawnstate},
spawnhealth: {spawnhealth},
seestate: {seestate},
seesound: {seesound},
reactiontime: {reactiontime},
attacksound: {attacksound},
painstate: {painstate},
painchance: {painchance},
painsound: {painsound},
meleestate: {meleestate},
missilestate: {missilestate},
deathstate: {deathstate},
xdeathstate: {xdeathstate},
deathsound: {deathsound},
speed: {speed},
radius: {radius},
height: {height},
mass: {mass},
damage: {damage},
activesound: {activesound},
flags: {flags},
raisestate: {raisestate},
}},"#,
        doomednum = info.get("doomednum").unwrap_or(&"-1".to_string()),
        spawnstate = info
            .get("spawnstate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        spawnhealth = info.get("spawnhealth").unwrap_or(&"0".to_string()),
        seestate = info
            .get("seestate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        seesound = info
            .get("seesound")
            .map(|n| if n == "0" { "SfxEnum::None" } else { n })
            .unwrap_or("SfxEnum::None"),
        reactiontime = info.get("reactiontime").unwrap_or(&"0".to_string()),
        attacksound = info
            .get("attacksound")
            .map(|n| if n == "0" { "SfxEnum::None" } else { n })
            .unwrap_or("SfxEnum::None"),
        painstate = info
            .get("painstate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        painchance = info.get("painchance").unwrap_or(&"0".to_string()),
        painsound = info
            .get("painsound")
            .map(|n| if n == "0" { "SfxEnum::None" } else { n })
            .unwrap_or("SfxEnum::None"),
        meleestate = info
            .get("meleestate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        missilestate = info
            .get("missilestate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        deathstate = info
            .get("deathstate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        xdeathstate = info
            .get("xdeathstate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        deathsound = info
            .get("deathsound")
            .map(|n| if n == "0" { "SfxEnum::None" } else { n })
            .unwrap_or("SfxEnum::None"),
        speed = info
            .get("speed")
            .map(|n| if !n.contains(".0") {
                format!("{n}.0")
            } else {
                n.to_string()
            })
            .map(|n| if n == "0" {
                "0.0".to_string()
            } else {
                n.to_string()
            })
            .unwrap_or_else(|| "0.0".to_string()),
        radius = info.get("radius").unwrap_or(&"0.0".to_string()),
        height = info.get("height").unwrap_or(&"0.0".to_string()),
        mass = info.get("mass").unwrap_or(&"0".to_string()),
        damage = info.get("damage").unwrap_or(&"0".to_string()),
        activesound = info
            .get("activesound")
            .map(|n| if n == "0" { "SfxEnum::None" } else { n })
            .unwrap_or("SfxEnum::None"),
        flags = info.get("flags").unwrap_or(&"0".to_string()),
        raisestate = info
            .get("raisestate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
    )
}

#[cfg(test)]
mod tests {
    use crate::{info_to_string, parse_info, read_file};
    use std::path::PathBuf;

    #[test]
    fn test_info() {
        let data = read_file(PathBuf::from("multigen.txt.orig"));
        let (order, info) = parse_info(&data);

        let plasma = info.get("MT_PLASMA").unwrap();
        assert_eq!(plasma.get("spawnstate").unwrap(), "StateNum::S_PLASBALL");
        assert_eq!(plasma.get("deathstate").unwrap(), "StateNum::S_PLASEXP");

        let lines = info_to_string("MT_PLASMA", &plasma);
        dbg!(&lines);
    }
}
