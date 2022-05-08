use crate::Game;
use game_traits::{GameState, GameTraits};
use gameplay::{GameAction, GameMode, Skill, WBPlayerStruct, WBStartStruct};
use sound_traits::{MusEnum, SfxNum, SoundAction, EPISODE4_MUS};

impl GameTraits for Game {
    /// G_InitNew
    /// Can be called by the startup code or the menu task,
    /// consoleplayer, displayplayer, playeringame[] should be set.
    ///
    /// This appears to be defered because the function call can happen at any time
    /// in the game-exe. So rather than just abruptly stop everything we should set
    /// the action so that the right sequences are run. Unsure of impact of
    /// changing game-exe vars beyong action here, probably nothing.
    fn defered_init_new(&mut self, skill: Skill, episode: i32, map: i32) {
        self.game_skill = skill;
        self.game_episode = episode;
        self.game_map = map;
        self.game_action = GameAction::NewGame;
    }

    fn get_mode(&mut self) -> GameMode {
        self.game_mode
    }

    fn load_game(&mut self, name: String) {
        todo!()
    }

    fn save_game(&mut self, name: String, slot: usize) {
        todo!()
    }

    fn toggle_pause_game(&mut self) {
        self.paused = !self.paused;
    }

    fn quit_game(&mut self) {
        self.set_running(false);
    }

    fn start_sound(&mut self, sfx: SfxNum) {
        let sfx = SoundAction::StartSfx {
            uid: 0,
            sfx,
            x: 0.0,
            y: 0.0,
        };
        self.snd_command.send(sfx).unwrap();
    }

    fn change_music(&mut self, mus: MusEnum) {
        let music = if mus == MusEnum::None {
            if self.game_mode == GameMode::Commercial {
                MusEnum::Runnin as usize + self.game_map as usize - 1
            } else if self.game_episode < 4 {
                MusEnum::E1M1 as usize
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
    fn world_done(&mut self) {
        self.game_action = GameAction::WorldDone;
        if let Some(level) = &self.level {
            if level.secret_exit {
                for p in self.players.iter_mut() {
                    p.didsecret = true;
                }
            }
            if matches!(self.game_mode, GameMode::Commercial) {
                match self.game_map {
                    6 | 11 | 15 | 20 | 30 | 31 => {
                        // if !level.secret_exit && (self.game_map == 15 || self.game_map == 31) {
                        //     // ignore
                        // } else {
                        //     // TODO: F_StartFinale();
                        // }
                    }
                    _ => {}
                }
            }
        }
    }

    fn level_end_info(&self) -> &WBStartStruct {
        &self.wminfo
    }

    fn player_end_info(&self) -> &WBPlayerStruct {
        &self.wminfo.plyr[self.consoleplayer]
    }

    fn set_game_state(&mut self, state: GameState) {
        self.game_state = state;
    }

    fn get_game_state(&mut self) {}
}
