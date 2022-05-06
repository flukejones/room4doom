use crate::Game;
use gameplay::{GameAction, Skill};
use menu_traits::MenuFunctions;
use sdl2::libc::pause;

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

    fn load_game(&mut self, name: String) {
        todo!()
    }

    fn save_game(&mut self, name: String, slot: usize) {
        todo!()
    }

    fn pause_game(&mut self, pause: bool) {
        self.paused = pause;
    }

    fn quit_game(&mut self) {
        self.set_running(false);
    }
}
