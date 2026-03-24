use crate::Game;
use game_config::{GameMode, Skill};
use gameplay::{GameAction, PlayerStatus, WorldEndPlayerInfo, save};
use gamestate_traits::{ConfigKey, ConfigTraits, GameState, GameTraits, WorldInfo};
use sound_common::{
    EPISODE4_MUS, MID_ID, MUS_ID, MusTrack, SfxName, SoundAction, read_mus_to_midi
};
use wad::WadData;

impl GameTraits for Game {
    /// G_InitNew
    /// Can be called by the startup code or the menu task,
    /// consoleplayer, displayplayer, playeringame[] should be set.
    ///
    /// This appears to be defered because the function call can happen at any
    /// time in the game-exe. So rather than just abruptly stop everything
    /// we should set the action so that the right sequences are run. Unsure
    /// of impact of changing game-exe vars beyong action here, probably
    /// nothing.
    fn defered_init_new(&mut self, skill: Skill, episode: usize, map: usize) {
        self.options.skill = skill;
        self.options.episode = episode;
        self.options.map = map;
        self.pending_action = GameAction::NewGame;
    }

    fn get_mode(&self) -> GameMode {
        self.game_type.mode
    }

    fn game_state(&self) -> GameState {
        self.gamestate
    }

    fn read_save_descriptions(&self) -> Vec<Option<String>> {
        let dir = Self::save_dir();
        (0..6)
            .map(|i| {
                let path = dir.join(format!("slot{i}.sav"));
                let data = std::fs::read(&path).ok()?;
                let header = save::parse_save_header(&data).ok()?;
                if header.description.is_empty() {
                    Some(header.map_name)
                } else {
                    Some(header.description)
                }
            })
            .collect()
    }

    fn load_game(&mut self, name: String) {
        self.save_name = Some(name);
        self.pending_action = GameAction::LoadGame;
    }

    fn save_game(&mut self, name: String, description: String) {
        self.save_description = description;
        self.save_name = Some(name);
        self.pending_action = GameAction::SaveGame;
    }

    fn toggle_pause_game(&mut self) {
        self.paused = !self.paused;
    }

    fn quit_game(&mut self) {
        self.set_running(false);
    }

    fn start_sound(&mut self, sfx: SfxName) {
        let sfx = SoundAction::StartSfx {
            uid: 0,
            sfx,
            x: 0.0,
            y: 0.0,
        };
        self.sound_cmd.send(sfx).unwrap();
    }

    fn change_music(&self, mus: MusTrack) {
        let track = if mus == MusTrack::None {
            if self.game_type.mode == GameMode::Commercial {
                MusTrack::from((MusTrack::Runnin as u8) + (self.options.map as u8) - 1)
            } else if self.options.episode < 4 {
                MusTrack::from(
                    (MusTrack::E1M1 as u8)
                        + ((self.options.episode as u8 - 1) * 9)
                        + (self.options.map as u8)
                        - 1,
                )
            } else {
                EPISODE4_MUS[self.options.map - 1]
            }
        } else {
            mus
        };
        self.send_music_lump(&track.lump_name());
    }

    fn change_music_by_lump(&self, lump_name: &str) {
        self.send_music_lump(lump_name);
    }

    /// Doom function name `G_WorldDone`
    fn level_done(&mut self) {
        self.pending_action = GameAction::WorldDone;
        if self.world_info.didsecret {
            for p in self.players.iter_mut() {
                p.didsecret = true;
            }
        }
        if matches!(self.game_type.mode, GameMode::Commercial) {
            match self.world_info.last {
                6 | 11 | 15 | 20 | 30 | 31 => {
                    if !self.world_info.didsecret
                        && (self.options.map == 15 || self.options.map == 31)
                    {
                        return;
                    }
                    self.world_info.didsecret = self.players[self.consoleplayer].didsecret;
                    self.pending_action = GameAction::Victory;
                }
                _ => {}
            }
        }
    }

    fn finale_done(&mut self) {
        self.pending_action = GameAction::WorldDone;
    }

    fn start_title(&mut self) {
        self.start_title();
    }

    fn level_end_info(&self) -> &WorldInfo {
        &self.world_info
    }

    fn player_end_info(&self) -> &WorldEndPlayerInfo {
        &self.world_info.plyr[self.consoleplayer]
    }

    fn player_status(&self) -> PlayerStatus {
        self.players[self.consoleplayer].status.clone()
    }

    fn player_msg_take(&mut self) -> Option<String> {
        self.players[self.consoleplayer]
            .message
            .take()
            .map(|s| s.to_string())
    }

    fn get_wad_data(&self) -> &WadData {
        &self.wad_data
    }
}

impl ConfigTraits for Game {
    fn config_value(&self, key: ConfigKey) -> i32 {
        self.config_values[key as usize]
    }

    fn set_config_value(&mut self, key: ConfigKey, val: i32) {
        self.config_values[key as usize] = val;
        match key {
            ConfigKey::SfxVolume => {
                let _ = self.sound_cmd.send(SoundAction::SfxVolume(val));
            }
            ConfigKey::MusVolume => {
                let _ = self.sound_cmd.send(SoundAction::MusicVolume(val));
            }
            _ => {}
        }
    }

    fn mark_config_changed(&mut self) {
        self.config_dirty = true;
    }

    fn is_config_dirty(&self) -> bool {
        self.config_dirty
    }

    fn clear_config_dirty(&mut self) {
        self.config_dirty = false;
    }

    fn config_snapshot(&self) -> [i32; ConfigKey::KeyCount as usize] {
        self.config_values
    }
}

impl Game {
    /// Replay the current map's music track, respecting UMAPINFO overrides.
    pub fn replay_current_music(&self) {
        let map_name = self.current_map_name();
        let map_entry = self.umapinfo.as_ref().and_then(|u| u.get(&map_name));
        if let Some(music) = map_entry.and_then(|e| e.music.as_deref()) {
            self.send_music_lump(music);
        } else {
            use gamestate_traits::GameTraits;
            self.change_music(MusTrack::None);
        }
    }

    pub(crate) fn send_music_lump(&self, lump_name: &str) {
        if let Some(data) = self.music_data_for_lump(lump_name) {
            self.sound_cmd
                .send(SoundAction::ChangeMusic(data, true))
                .unwrap();
        } else {
            log::warn!("Music lump '{}' not found or empty", lump_name);
        }
    }

    fn music_data_for_lump(&self, lump_name: &str) -> Option<Vec<u8>> {
        let lump = self.wad_data.get_lump(lump_name)?;
        if lump.data.len() < 4 {
            return None;
        }
        if lump.data[..4] == MUS_ID {
            read_mus_to_midi(&lump.data)
        } else if lump.data[..4] == MID_ID {
            Some(lump.data.clone())
        } else {
            Some(lump.data.clone())
        }
    }
}
