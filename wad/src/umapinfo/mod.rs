mod mapinfo;
mod parse;

use std::collections::HashMap;

pub use mapinfo::parse_mapinfo;
pub use parse::ParseError;

#[derive(Debug, Clone)]
pub struct UMapInfo {
    pub(crate) entries: Vec<MapEntry>,
    pub(crate) index: HashMap<String, usize>,
    pub clear_episodes: bool,
}

impl UMapInfo {
    pub fn get(&self, map_name: &str) -> Option<&MapEntry> {
        let key = map_name.to_ascii_uppercase();
        self.index.get(&key).map(|&i| &self.entries[i])
    }

    pub fn get_by_ep_map(&self, episode: usize, map: usize) -> Option<&MapEntry> {
        self.entries
            .iter()
            .find(|e| e.episode == episode && e.map == map)
    }

    pub fn episodes(&self) -> Vec<&EpisodeDef> {
        self.entries
            .iter()
            .filter_map(|e| e.episode_def.as_ref())
            .collect()
    }

    pub fn entries(&self) -> &[MapEntry] {
        &self.entries
    }
}

#[derive(Debug, Clone, Default)]
pub struct MapEntry {
    pub map_name: String,
    pub episode: usize,
    pub map: usize,
    pub level_name: Option<String>,
    pub label: Option<LabelKind>,
    pub author: Option<String>,
    pub level_pic: Option<String>,
    pub next: Option<String>,
    pub next_secret: Option<String>,
    pub sky_texture: Option<String>,
    pub music: Option<String>,
    pub exit_pic: Option<String>,
    pub enter_pic: Option<String>,
    pub par_time: Option<i32>,
    pub end_game: Option<bool>,
    pub end_pic: Option<String>,
    pub end_bunny: bool,
    pub end_cast: bool,
    pub no_intermission: bool,
    pub inter_text: Option<TextOrClear>,
    pub inter_text_secret: Option<TextOrClear>,
    pub inter_backdrop: Option<String>,
    pub inter_music: Option<String>,
    pub episode_def: Option<EpisodeDef>,
    pub boss_actions: Option<BossActions>,
    pub cluster_id: Option<i32>,
}

#[derive(Debug, Clone)]
pub enum LabelKind {
    Text(String),
    Clear,
}

#[derive(Debug, Clone)]
pub enum TextOrClear {
    Text(String),
    Clear,
}

#[derive(Debug, Clone)]
pub struct EpisodeDef {
    pub patch: String,
    pub name: String,
    pub key: String,
}

#[derive(Debug, Clone)]
pub enum BossActions {
    Clear,
    Actions(Vec<BossAction>),
}

#[derive(Debug, Clone)]
pub struct BossAction {
    pub thing_type: String,
    pub line_special: i32,
    pub tag: i32,
}

pub fn parse(input: &str) -> Result<UMapInfo, ParseError> {
    parse::parse(input)
}

/// Parse map name into (episode, map) numbers.
/// "E6M1" → (6, 1), "MAP01" → (0, 1), "MAP20" → (0, 20)
/// Episode 0 means commercial format (MAPxx).
pub fn parse_map_name(name: &str) -> (usize, usize) {
    let upper = name.to_ascii_uppercase();
    if upper.starts_with('E') {
        if let Some(m_pos) = upper.find('M') {
            let ep = upper[1..m_pos].parse().unwrap_or(0);
            let map = upper[m_pos + 1..].parse().unwrap_or(0);
            return (ep, map);
        }
    }
    if upper.starts_with("MAP") {
        let map = upper[3..].parse().unwrap_or(0);
        return (0, map);
    }
    (0, 0)
}
