use super::{Intermission, State};
use gamestate_traits::GameTraits;
use log::info;
use render_common::DrawBuffer;

impl Intermission {
    pub(super) fn draw_no_state(
        &mut self,
        x_ofs: f32,
        sx: f32,
        sy: f32,
        pixels: &mut impl DrawBuffer,
    ) {
        self.pointer_on = true;
        self.draw_next_loc_pixels(x_ofs, sx, sy, pixels);
    }

    pub(super) fn init_no_state(&mut self) {
        self.state = State::None;
        self.count = 10;
    }

    pub(super) fn update_no_state(&mut self, game: &mut impl GameTraits) {
        self.update_animated_bg();

        let player = &self.player_info;
        let level = &self.level_info;

        self.count -= 1;
        if self.count <= 0 {
            info!(
                "Player: Total Items: {}/{}",
                player.items_collected, level.maxitems
            );
            info!(
                "Player: Total Kills: {}/{}",
                player.total_kills, level.maxkills
            );
            info!(
                "Player: Total Secrets: {}/{}",
                player.secrets_found, level.maxsecret
            );
            info!("Player: Level Time: {}", player.level_time);
            game.level_done();
        }
    }
}
