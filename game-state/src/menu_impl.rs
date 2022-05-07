use crate::Game;
use gameplay::{GameAction, GameMode, Skill};
use menu_traits::MenuFunctions;
use sdl2::libc::pause;
use sound_traits::{MusEnum, SfxNum, SoundAction};

impl MenuFunctions for Game {
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
}