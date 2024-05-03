use log::info;

use crate::doom_def::GameMode;
use crate::level::map_defs::LineDef;
use crate::{DPtr, PicData};

#[derive(Debug)]
pub enum ButtonWhere {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug)]
pub struct Button {
    pub line: DPtr<LineDef>,
    pub bwhere: ButtonWhere,
    pub texture: usize,
    pub timer: u32,
    // TODO: degenmobj_t *soundorg;
}

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
    /// Doom function name `P_InitSwitchList`
    pub fn init(game_mode: GameMode, pic_data: &PicData) -> Vec<usize> {
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
        info!("Initialised switch list");

        switch_list
    }
}
