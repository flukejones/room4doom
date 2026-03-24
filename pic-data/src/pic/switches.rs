use log::{info, warn};

use super::PicData;
use game_config::GameMode;
use wad::WadData;

struct ButtonDef {
    name1: &'static str,
    name2: &'static str,
    episode: i32,
}

impl ButtonDef {
    const fn new(name1: &'static str, name2: &'static str, episode: i32) -> Self {
        ButtonDef {
            name1,
            name2,
            episode,
        }
    }
}

// CHANGE THE TEXTURE OF A WALL SWITCH TO ITS OPPOSITE
const BUTTON_DEFS: [ButtonDef; 40] = [
    // Doom shareware episode 1 switches
    ButtonDef::new("SW1BRCOM", "SW2BRCOM", 1),
    ButtonDef::new("SW1BRN1", "SW2BRN1", 1),
    ButtonDef::new("SW1BRN2", "SW2BRN2", 1),
    ButtonDef::new("SW1BRNGN", "SW2BRNGN", 1),
    ButtonDef::new("SW1BROWN", "SW2BROWN", 1),
    ButtonDef::new("SW1COMM", "SW2COMM", 1),
    ButtonDef::new("SW1COMP", "SW2COMP", 1),
    ButtonDef::new("SW1DIRT", "SW2DIRT", 1),
    ButtonDef::new("SW1EXIT", "SW2EXIT", 1),
    ButtonDef::new("SW1GRAY", "SW2GRAY", 1),
    ButtonDef::new("SW1GRAY1", "SW2GRAY1", 1),
    ButtonDef::new("SW1METAL", "SW2METAL", 1),
    ButtonDef::new("SW1PIPE", "SW2PIPE", 1),
    ButtonDef::new("SW1SLAD", "SW2SLAD", 1),
    ButtonDef::new("SW1STARG", "SW2STARG", 1),
    ButtonDef::new("SW1STON1", "SW2STON1", 1),
    ButtonDef::new("SW1STON2", "SW2STON2", 1),
    ButtonDef::new("SW1STONE", "SW2STONE", 1),
    ButtonDef::new("SW1STRTN", "SW2STRTN", 1),
    // Doom registered episodes 2&3 switches
    ButtonDef::new("SW1BLUE", "SW2BLUE", 2),
    ButtonDef::new("SW1CMT", "SW2CMT", 2),
    ButtonDef::new("SW1GARG", "SW2GARG", 2),
    ButtonDef::new("SW1GSTON", "SW2GSTON", 2),
    ButtonDef::new("SW1HOT", "SW2HOT", 2),
    ButtonDef::new("SW1LION", "SW2LION", 2),
    ButtonDef::new("SW1SATYR", "SW2SATYR", 2),
    ButtonDef::new("SW1SKIN", "SW2SKIN", 2),
    ButtonDef::new("SW1VINE", "SW2VINE", 2),
    ButtonDef::new("SW1WOOD", "SW2WOOD", 2),
    // Doom II switches
    ButtonDef::new("SW1PANEL", "SW2PANEL", 3),
    ButtonDef::new("SW1ROCK", "SW2ROCK", 3),
    ButtonDef::new("SW1MET2", "SW2MET2", 3),
    ButtonDef::new("SW1WDMET", "SW2WDMET", 3),
    ButtonDef::new("SW1BRIK", "SW2BRIK", 3),
    ButtonDef::new("SW1MOD1", "SW2MOD1", 3),
    ButtonDef::new("SW1ZIM", "SW2ZIM", 3),
    ButtonDef::new("SW1STON6", "SW2STON6", 3),
    ButtonDef::new("SW1TEK", "SW2TEK", 3),
    ButtonDef::new("SW1MARB", "SW2MARB", 3),
    ButtonDef::new("SW1SKULL", "SW2SKULL", 3),
];

pub struct Switches;

impl Switches {
    /// Initialise switch texture pairs from WAD data. OG: P_InitSwitchList.
    pub fn init(game_mode: GameMode, pic_data: &PicData, wad: &WadData) -> Vec<usize> {
        let episode = match game_mode {
            GameMode::Registered | GameMode::Retail => 2,
            GameMode::Commercial => 3,
            _ => 1,
        };

        let mut switch_list = Vec::new();
        for def in BUTTON_DEFS {
            if def.episode <= episode {
                switch_list.push(
                    pic_data
                        .wallpic_num_for_name(def.name1)
                        .unwrap_or_else(|| panic!("No texture for {}", def.name1)),
                );
                switch_list.push(
                    pic_data
                        .wallpic_num_for_name(def.name2)
                        .unwrap_or_else(|| panic!("No texture for {}", def.name2)),
                );
            }
        }

        if let Some(lump) = wad.get_lump("SWITCHES") {
            let boom_entries = wad::boom::parse_switches(&lump.data);
            let mut added = 0;
            for entry in &boom_entries {
                if entry.episode as i32 > episode {
                    continue;
                }
                let (Some(tex1), Some(tex2)) = (
                    pic_data.wallpic_num_for_name(&entry.name1),
                    pic_data.wallpic_num_for_name(&entry.name2),
                ) else {
                    warn!(
                        "SWITCHES lump: missing texture {} or {}",
                        entry.name1, entry.name2
                    );
                    continue;
                };
                if !switch_list.contains(&tex1) {
                    switch_list.push(tex1);
                    switch_list.push(tex2);
                    added += 1;
                }
            }
            if added > 0 {
                info!(
                    "Extended switch list with {} entries from SWITCHES lump",
                    added
                );
            }
        }

        info!("Initialised switch list ({} pairs)", switch_list.len() / 2);
        switch_list
    }
}
