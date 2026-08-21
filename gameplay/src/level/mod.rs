//! The data that makes up an entire level, along with functions to record
//! state, or get ref/mutable-ref to parts of it.
//!
//! Some of the state is mirrored from the overall game-exe state, or ref by
//! pointer.
#[cfg(test)]
pub mod tests;

use std::collections::VecDeque;
use std::ptr;

use level::{LevelData, LineDef, MapPtr};
use sound_common::{SfxName, SndServerTx, SoundAction};
use wad::WadData;
use wad::types::WadThing;

use game_config::{GameMode, GameOptions};

use pic_data::{PicAnimation, PicData, Switches};

use crate::Player;
use crate::doom_def::{GameAction, MAX_DEATHMATCH_STARTS, MAX_RESPAWNS, MAXPLAYERS};
use crate::env::effects::{MoveEffect, MoverPhase};
use crate::pic::Button;
use crate::thing::MapObject;
use crate::thinker::ThinkerAlloc;

/// The world the player is in; exists only while the player is in it.
///
/// Groups the interlinked level data so thinkers and functions can work
/// with it together.
pub struct LevelState {
    pub map_name: String,
    /// All the data required to build and display a level
    pub level_data: LevelData,
    /// Thinkers are objects that are not static, like enemies, switches,
    /// platforms, lights etc
    pub thinkers: ThinkerAlloc,
    pub respawn_queue: VecDeque<(u32, WadThing)>,

    pub options: GameOptions,

    pub level_timer: bool,
    /// Time spent in level
    pub level_time: u32,

    /// The `Things` for player start locations
    pub player_starts: [Option<WadThing>; MAXPLAYERS],
    /// The `Things` for deathmatch start locations
    pub(super) deathmatch_starts: [Option<WadThing>; MAX_DEATHMATCH_STARTS],
    pub(super) deathmatch_p: Vec<WadThing>,

    /// for intermission
    pub total_level_kills: i32,
    /// for intermission
    pub total_level_items: i32,
    /// for intermission
    pub total_level_secrets: i32,
    /// To change the game-exe state via switches in the level
    /// Record how the level was exited
    pub secret_exit: bool,

    pub game_action: Option<GameAction>,

    /// Pre-composed textures, shared to the renderer. `doom-lib` owns and uses
    /// access to change animations + translation tables.
    /// Pre-generated texture animations
    pub animations: Vec<PicAnimation>,
    /// List of switch textures in ordered pairs
    pub switch_list: Vec<usize>,
    /// Wall texture pixel heights indexed by texture number (OG `textureheight`)
    pub(crate) texture_heights: Vec<i32>,
    /// List of used buttons. Typically these buttons or switches are timed.
    pub(crate) button_list: Vec<Button>,
    /// Scrolling walls
    pub(super) line_special_list: Vec<MapPtr<LineDef>>,

    /// Tracks which players are currently active, set by d_net.c loop.
    /// This is a raw pointer to the array in `Game`, and must not be modified
    players_in_game: *const [bool; MAXPLAYERS],
    /// Each player in the array may be controlled.
    /// This is a raw pointer to the array in `Game`, and must not be modified
    players: *mut [Player; MAXPLAYERS],

    /// Some stuff needs to know the game-exe mode (e.g, switching weapons)
    pub(super) game_mode: GameMode,

    /// Marker count for lines checked
    pub(super) valid_count: usize,

    /// Provides ability for things to start a sound
    pub(super) snd_command: SndServerTx,

    /// Movers that a tagged stop/start special can put in or out of stasis.
    pub(crate) active_movers: Vec<*mut MoveEffect>,
    pub(crate) sky_num: usize,
    /// OG Doom blocklinks: per-block linked list heads for thing iteration
    pub(crate) blocklinks: Vec<Option<*mut MapObject>>,
    /// UMAPINFO boss death actions for this map (None = use defaults)
    pub boss_actions: Option<wad::umapinfo::BossActions>,
}

impl LevelState {
    /// Set up a complete level including difficulty, spawns, players etc.
    /// After `new()` the `load()` function should be called.
    ///
    /// # Safety
    /// Because the `LevelState` uses ` ThinkerAlloc` internally the
    /// `LevelState` must not be moved by the owner after any thinkers are
    /// pushed to `ThinkerAlloc`. This applies to the map data also where
    /// `load()` should be called after the locations is set in concrete.
    /// Other common tasks to do after `new()` are spawning specials,
    /// things.
    ///
    /// Doom method name is `P_SetupLevel`
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn new_empty(
        options: GameOptions,
        game_mode: GameMode,
        snd_command: SndServerTx,
        players_in_game: &[bool; MAXPLAYERS],
        players: &mut [Player; MAXPLAYERS],
    ) -> Self {
        let level_data = LevelData::default();

        Self {
            map_name: String::new(),
            level_data,
            // SAFETY: The level owns this fixed thinker allocation; game-owned player arrays
            // outlive the level and spawning happens after initialization.
            thinkers: unsafe { ThinkerAlloc::new(0) },
            options,
            respawn_queue: VecDeque::with_capacity(MAX_RESPAWNS),
            level_time: 0,
            level_timer: false,
            player_starts: [None; MAXPLAYERS],
            deathmatch_starts: [None; MAX_DEATHMATCH_STARTS],
            deathmatch_p: Vec::with_capacity(MAX_DEATHMATCH_STARTS),
            total_level_kills: 0,
            total_level_items: 0,
            total_level_secrets: 0,
            game_action: None,
            secret_exit: false,
            valid_count: 0,
            switch_list: Default::default(),
            texture_heights: Default::default(),
            animations: Default::default(),
            button_list: Vec::with_capacity(50),
            line_special_list: Vec::with_capacity(50),
            game_mode,
            snd_command,
            players_in_game,
            players,
            active_movers: Vec::new(),
            sky_num: 0,
            blocklinks: Vec::new(),
            boss_actions: None,
        }
    }

    pub(crate) fn stop_movers(&mut self, tag: i16) {
        for mover in &mut self.active_movers {
            // SAFETY: Active movers live in fixed thinker slots and are removed from the active
            // list before their slots are released.
            let mover = unsafe { &mut **mover };
            if mover.tag == tag && mover.phase != MoverPhase::InStasis {
                mover.stasis_phase = mover.phase;
                mover.phase = MoverPhase::InStasis;
            }
        }
    }

    pub(crate) fn activate_movers_in_stasis(&mut self, tag: i16) {
        for mover in &mut self.active_movers {
            // SAFETY: Active movers live in fixed thinker slots and are removed from the active
            // list before their slots are released.
            let mover = unsafe { &mut **mover };
            if mover.tag == tag && mover.phase == MoverPhase::InStasis {
                mover.phase = mover.stasis_phase;
            }
        }
    }

    pub(crate) fn add_active_mover(&mut self, mover: *mut MoveEffect) {
        self.active_movers.push(mover);
    }

    /// Clear active movers list. Used by save/load.
    pub fn clear_active_movers(&mut self) {
        self.active_movers.clear();
    }

    /// Drop a finished mover from the stasis list and mark its thinker for
    /// removal.
    ///
    /// # Safety
    /// The mover *must* be live. For example do not call `.mark_remove()`
    /// before `remove_active_mover()`.
    pub(crate) unsafe fn remove_active_mover(&mut self, mover: &mut MoveEffect) {
        if let Some(index) = self.active_movers.iter().position(|p| ptr::eq(*p, mover)) {
            self.active_movers.remove(index);
        }
        // SAFETY: Active movers live in fixed thinker slots and are removed from the active
        // list before their slots are released.
        unsafe { &mut *mover.thinker }.mark_remove();
    }

    pub(super) fn players_in_game(&self) -> &[bool; MAXPLAYERS] {
        // SAFETY: The game owns these fixed player arrays and keeps them alive at the addresses
        // supplied when the level is constructed.
        unsafe { &*self.players_in_game }
    }

    pub(super) fn players(&self) -> &[Player; MAXPLAYERS] {
        // SAFETY: The game owns these fixed player arrays and keeps them alive at the addresses
        // supplied when the level is constructed.
        unsafe { &*self.players }
    }

    pub(super) fn players_mut(&mut self) -> &mut [Player; MAXPLAYERS] {
        // SAFETY: The game owns these fixed player arrays and keeps them alive at the addresses
        // supplied when the level is constructed.
        unsafe { &mut *self.players }
    }

    pub fn load(
        &mut self,
        map_name: &str,
        game_mode: GameMode,
        pic_data: &mut PicData,
        wad_data: &WadData,
        sky_override: Option<&str>,
    ) {
        let animations = PicAnimation::init(pic_data, wad_data);
        let switch_list = Switches::init(self.game_mode, pic_data, wad_data);

        pic_data.set_sky_pic(game_mode, self.options.episode, self.options.map);
        if let Some(sky) = sky_override {
            pic_data.set_sky_pic_by_name(sky);
        }
        self.sky_num = pic_data.sky_num();

        self.level_data.load(
            map_name,
            |name| pic_data.flat_num_for_name(name),
            wad_data,
            Some(self.sky_num),
            Some(pic_data.sky_pic()),
        );
        self.map_name = map_name.to_owned();
        self.animations = animations;
        self.switch_list = switch_list;
        self.texture_heights = (0..pic_data.num_textures())
            .map(|i| pic_data.texture_height(i))
            .collect();

        // Allocate blockmap thing chains
        let bm = self.level_data.blockmap();
        let num_blocks = (bm.columns * bm.rows) as usize;
        self.blocklinks = vec![None; num_blocks];

        // SAFETY: The level owns this fixed thinker allocation; game-owned player arrays
        // outlive the level and spawning happens after initialization.
        unsafe {
            self.thinkers = ThinkerAlloc::new(self.level_data.things().len() * 2 + 256);
        }
    }

    pub(super) const fn do_exit_level(&mut self) {
        self.secret_exit = false;
        self.game_action = Some(GameAction::CompletedLevel);
    }

    pub(crate) const fn do_secret_exit_level(&mut self) {
        self.secret_exit = true;
        self.game_action = Some(GameAction::CompletedLevel);
    }

    pub(super) const fn do_completed(&mut self) {
        self.secret_exit = false;
        self.game_action = Some(GameAction::Victory);
    }

    pub(super) fn start_sound(&self, sfx: SfxName, x: f32, y: f32, uid: usize) {
        if let Err(e) = self.snd_command.send(SoundAction::StartSfx {
            uid,
            sfx,
            x,
            y,
        }) {
            log::warn!("Could not send sfx, sound thread gone: {e}");
        }
    }
}
