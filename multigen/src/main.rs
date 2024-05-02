pub mod parse_info;
pub mod strings;

use crate::{
    parse_info::{info_to_string, state_to_string},
    strings::*,
};
use argh::FromArgs;
use std::{
    collections::HashMap,
    error::Error,
    fs::OpenOptions,
    io::{Read, Write},
    path::PathBuf,
};

// pub struct State {
//     /// Sprite to use
//     pub sprite: SpriteNum,
//     /// The frame within this sprite to show for the state
//     pub frame: u32,
//     /// How many tics this state takes. On nightmare it is shifted >> 1
//     pub tics: i32,
//     // void (*action) (): i32,
//     /// An action callback to run on this state
//     pub action: ActFn,
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

/// Turn a mapinfo file in to rust
#[derive(Debug, Clone, FromArgs)]
struct CLIOptions {
    /// path to info data
    #[argh(option)]
    info: PathBuf,
    /// path to write generated files to
    #[argh(option)]
    out: PathBuf,
}

pub type InfoType = HashMap<String, String>;
pub type InfoGroupType = HashMap<String, InfoType>;

fn main() -> Result<(), Box<dyn Error>> {
    let options: CLIOptions = argh::from_env();
    let data = read_file(options.info);

    // Lines starting with:
    // - `;` are comments
    // - `$` are MapObjInfo, and may not include all possible fields
    // - `S_` are `StateNum::S_*`, and `State`
    //
    // An `S_` is unique and should accumulate in order
    // `S_` line order: statename  sprite  frame tics action nextstate [optional1] [optional2]
    //
    // SfxName are pre-determined?

    let data = parse_data(&data);
    write_info_file(data, options.out);
    Ok(())
}

pub fn read_file(path: PathBuf) -> String {
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

pub fn write_info_file(data: Data, path: PathBuf) {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path.clone())
        .unwrap_or_else(|e| panic!("Couldn't open {:?}, {}", path, e));

    file.write_all(FILE_HEADER_STR.as_bytes()).unwrap();
    // SPRITE NAMES
    file.write_all(SPRITE_NAME_ARRAY_STR.as_bytes()).unwrap();
    for names in data.sprite_names.chunks(8) {
        let s: String = names
            .iter()
            .map(|s| format!("\"{s}\", "))
            .collect::<Vec<String>>()
            .concat();
        file.write_all(s.as_bytes()).unwrap();
        file.write_all(&[b'\n']).unwrap();
    }
    file.write_all(ARRAY_END_STR.as_bytes()).unwrap();

    // SPRITE ENUM
    file.write_all(SPRITE_ENUM_HEADER.as_bytes()).unwrap();
    for names in data.sprite_names.chunks(8) {
        let s: String = names
            .iter()
            .map(|s| format!("{s}, "))
            .collect::<Vec<String>>()
            .concat();
        file.write_all(s.as_bytes()).unwrap();
        file.write_all(&[b'\n']).unwrap();
    }
    file.write_all(SPRITE_ENUM_END.as_bytes()).unwrap();

    // STATE ENUM
    file.write_all(STATE_ENUM_HEADER.as_bytes()).unwrap();
    for names in data.state_order.chunks(8) {
        let s: String = names
            .iter()
            .map(|s| format!("{}, ", s.trim_start_matches("S_").replace("NULL", "None")))
            .collect::<Vec<String>>()
            .concat();
        file.write_all(s.as_bytes()).unwrap();
        file.write_all(&[b'\n']).unwrap();
    }
    file.write_all(STATE_ENUM_END.as_bytes()).unwrap();

    // MOBJ KIND ENUM
    file.write_all(MKIND_ENUM_HEADER.as_bytes()).unwrap();
    for names in data.mobj_order.chunks(8) {
        let s: String = names
            .iter()
            .map(|s| format!("{s}, "))
            .collect::<Vec<String>>()
            .concat();
        file.write_all(s.as_bytes()).unwrap();
        file.write_all(&[b'\n']).unwrap();
    }
    file.write_all(MKIND_ENUM_END.as_bytes()).unwrap();

    // STATES
    file.write_all(STATE_ARRAY_STR.as_bytes()).unwrap();
    for key in data.state_order.iter() {
        let value = data.states.get(key).unwrap();
        let info = state_to_string(key, value);
        file.write_all(info.as_bytes()).unwrap();
    }
    file.write_all(ARRAY_END_STR.as_bytes()).unwrap();

    // MOBJ INFO
    file.write_all(MOBJ_INFO_HEADER_STR.as_bytes()).unwrap();
    file.write_all(CLIPPY_ALLOW.as_bytes()).unwrap();
    file.write_all(MOBJ_INFO_TYPE_STR.as_bytes()).unwrap();
    file.write_all(MOBJ_INFO_ARRAY_STR.as_bytes()).unwrap();
    for key in data.mobj_order.iter() {
        let value = data.mobj_info.get(key).unwrap();
        let info = info_to_string(key, value);
        file.write_all(info.as_bytes()).unwrap();
    }
    file.write_all(ARRAY_END_STR.as_bytes()).unwrap();
}

pub struct Data {
    sprite_names: Vec<String>, // plain for sprnames, used for Enum also
    state_order: Vec<String>,
    states: InfoGroupType, // also convert to enum using key
    mobj_order: Vec<String>,
    mobj_info: InfoGroupType,
}

pub fn parse_data(input: &str) -> Data {
    // K/V = key/mobj name, <K= field, (data, comment)>
    let mut mobj_info: InfoGroupType = HashMap::new();
    let mut states: InfoGroupType = HashMap::new(); // Also used to build StateEnum

    let mut sprite_names = Vec::new();
    let mut state_order = Vec::new();
    let mut mobj_order = Vec::new();
    let mut info_misc_count = 0;
    let mut line_state = LineState::None;

    for line in input.lines() {
        if line.starts_with("S_") {
            let split: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
            if split.len() > 1 {
                states.insert(split[0].to_string(), HashMap::new());
                state_order.push(split[0].to_string());
                // Sprite enum
                if !sprite_names.contains(&split[1]) {
                    sprite_names.push(split[1].to_uppercase().to_string());
                }
                // State data
                if let Some(map) = states.get_mut(&split[0]) {
                    map.insert("sprite".to_string(), split[1].to_string());

                    let mut f = (split[2].to_uppercase().as_bytes()[0] - b'A') as u16;
                    if split[2].contains('*') {
                        f |= 0x8000;
                    }
                    map.insert("frame".to_string(), format!("{f}"));

                    map.insert(
                        "tics".to_string(),
                        split[3].trim_end_matches('*').to_string(),
                    );

                    map.insert(
                        "action".to_string(),
                        if split[4].to_lowercase().contains("null") {
                            "ActFn::N".to_string()
                        } else {
                            validate_field(&split[4])
                        },
                    );

                    map.insert("next_state".to_string(), validate_field(&split[5]));
                }
            }
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
                mobj_info.insert(split[1].clone(), HashMap::new());
                mobj_order.push(split[1].clone());
            } else {
                // Or one of:
                // if split[1] == "+" {
                // A misc object:
                // $ + doomednum 2023 spawnstate S_PSTR 	flags 	MF_SPECIAL|MF_COUNTITEM
                let mut map = HashMap::new();
                for chunk in split.chunks(2).skip(1) {
                    if chunk[0].starts_with(';') {
                        break;
                    }
                    map.insert(chunk[0].to_string(), validate_field(&chunk[1]));
                }
                let name = if split[1] == "+" {
                    let tmp = format!("MT_MISC{info_misc_count}");
                    info_misc_count += 1;
                    tmp
                } else {
                    split[1].to_string()
                };
                mobj_info.insert(name.clone(), map);
                mobj_order.push(name.clone());
                line_state = LineState::InfoType(name);
            }
        }

        // Multiline info
        if let LineState::InfoType(name) = &mut line_state {
            if line.is_empty() {
                // reset
                line_state = LineState::None;
                continue;
            }
            let split: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
            for chunk in split.chunks(2) {
                if chunk[0].starts_with(';') {
                    break;
                }
                if let Some(entry) = mobj_info.get_mut(name) {
                    entry.insert(chunk[0].clone(), validate_field(&chunk[1]));
                }
            }
        }
    }

    Data {
        sprite_names,
        state_order,
        states,
        mobj_order,
        mobj_info,
    }
}

pub fn validate_field(input: &str) -> String {
    if input.contains("*FRACUNIT") {
        // Convert to something we can parse with f32
        let mut tmp = input.trim_end_matches("*FRACUNIT").to_string();
        tmp.push_str(".0");
        tmp
    } else if input.starts_with("S_") {
        // Stat number
        let mut tmp = "StateNum::".to_string();
        let tmp2 = input
            .to_uppercase()
            .trim_start_matches("S_")
            .replace("NULL", "None");
        tmp.push_str(tmp2.as_str());
        tmp
    } else if input.starts_with("sfx_") {
        // Sound
        let mut tmp = "SfxName::".to_string();
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
    } else if input.starts_with("A_") {
        // Action function
        let lower = input.to_lowercase();
        if PLAYER_FUNCS.contains(&lower.as_str()) {
            format!("ActFn::P({lower})")
        } else {
            format!("ActFn::A({lower})")
        }
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

const PLAYER_FUNCS: [&str; 22] = [
    "a_bfgsound",
    "a_checkreload",
    "a_closeshotgun2",
    "a_firebfg",
    "a_firecgun",
    "a_firemissile",
    "a_firepistol",
    "a_fireplasma",
    "a_fireshotgun",
    "a_fireshotgun2",
    "a_gunflash",
    "a_light0",
    "a_light1",
    "a_light2",
    "a_loadshotgun2",
    "a_lower",
    "a_openshotgun2",
    "a_punch",
    "a_raise",
    "a_refire",
    "a_saw",
    "a_weaponready",
];
