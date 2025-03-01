use crate::Game;
use gameplay::{GameAction, GameMode, Skill, WorldEndPlayerInfo};
use gamestate_traits::{GameTraits, PlayerStatus, WorldInfo};
use sound_traits::{EPISODE4_MUS, MusTrack, SfxName, SoundAction};
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

    fn load_game(&mut self, _name: String) {
        todo!()
    }

    fn save_game(&mut self, _name: String, _slot: usize) {
        todo!()
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
        let music = if mus == MusTrack::None {
            if self.game_type.mode == GameMode::Commercial {
                MusTrack::Runnin as usize + self.options.map - 1
            } else if self.options.episode < 4 {
                MusTrack::E1M1 as usize + (self.options.episode - 1) * 9 + self.options.map - 1
            } else {
                EPISODE4_MUS[self.options.map - 1] as usize
            }
        } else {
            mus as usize
        };

        self.sound_cmd
            .send(SoundAction::ChangeMusic(music, true))
            .unwrap();
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
