use crate::Game;
use gameplay::{GameAction, GameMode, Skill, WBPlayerStruct, WBStartStruct};
use gamestate_traits::{GameTraits, PlayerStatus};
use sound_traits::{MusTrack, SfxName, SoundAction, EPISODE4_MUS};
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
        self.game_skill = skill;
        self.game_episode = episode;
        self.game_map = map;
        self.game_action = GameAction::NewGame;
    }

    fn get_mode(&self) -> GameMode {
        self.game_mode
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
        self.snd_command.send(sfx).unwrap();
    }

    fn change_music(&self, mus: MusTrack) {
        let music = if mus == MusTrack::None {
            if self.game_mode == GameMode::Commercial {
                MusTrack::Runnin as usize + self.game_map as usize - 1
            } else if self.game_episode < 4 {
                MusTrack::E1M1 as usize
                    + (self.game_episode as usize - 1) * 9
                    + self.game_map as usize
                    - 1
            } else {
                EPISODE4_MUS[self.game_map as usize - 1] as usize
            }
        } else {
            mus as usize
        };

        self.snd_command
            .send(SoundAction::ChangeMusic(music, true))
            .unwrap();
    }

    /// Doom function name `G_WorldDone`
    fn level_done(&mut self) {
        self.game_action = GameAction::WorldDone;
        if self.wminfo.didsecret {
            for p in self.players.iter_mut() {
                p.didsecret = true;
            }
        }
        if matches!(self.game_mode, GameMode::Commercial) {
            match self.wminfo.last {
                6 | 11 | 15 | 20 | 30 | 31 => {
                    if !self.wminfo.didsecret && (self.game_map == 15 || self.game_map == 31) {
                        return;
                    }
                    self.wminfo.didsecret = self.players[self.consoleplayer].didsecret;
                    self.game_action = GameAction::Victory;
                }
                _ => {}
            }
        }
    }

    fn finale_done(&mut self) {
        self.game_action = GameAction::WorldDone;
    }

    fn level_end_info(&self) -> &WBStartStruct {
        &self.wminfo
    }

    fn player_end_info(&self) -> &WBPlayerStruct {
        &self.wminfo.plyr[self.consoleplayer]
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
