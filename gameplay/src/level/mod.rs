//! The data that makes up an entire level, along with functions to record
//! state, or get ref/mutable-ref to parts of it.
//!
//! Some of the state is mirrored from the overall game-exe state, or ref by
//! pointer.

pub mod flags;
pub mod map_data;
pub mod map_defs;
pub mod node;

use std::collections::VecDeque;
use std::ptr;

use sound_sdl2::SndServerTx;
use sound_traits::{SfxName, SoundAction};
use wad::WadData;
use wad::types::WadThing;

use crate::doom_def::{GameAction, GameMode, MAX_DEATHMATCH_STARTS, MAX_RESPAWNS, MAXPLAYERS};
use crate::env::platforms::{PlatStatus, Platform};
use crate::level::map_data::MapData;
use crate::pic::Button;
use crate::thinker::ThinkerAlloc;
use crate::{GameOptions, MapPtr, PicAnimation, PicData, Player, Switches};

use self::map_defs::LineDef;

/// The level is considered a `World` or sorts. One that exists only
/// while the player is in it. Another benefit of this structure is
/// it makes it easier for all involved thinkers and functions to
/// work with the data, as much of it is interlinked.
pub struct Level {
    /// All the data required to build and display a level
    pub map_data: MapData,
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
    /// List of used buttons. Typically these buttons or switches are timed.
    pub(super) button_list: Vec<Button>,
    pub(super) line_special_list: Vec<MapPtr<LineDef>>,
    /// Provides ability for things to start a sound
    pub(super) snd_command: SndServerTx,

    active_platforms: Vec<*mut Platform>,
    pub(crate) sky_num: usize,
}

impl Level {
    /// Set up a complete level including difficulty, spawns, players etc.
    /// After `new()` the `load()` function should be called.
    ///
    /// # Safety
    /// Because the `Level` uses ` ThinkerAlloc` internally the `Level` must not
    /// be moved by the owner after any thinkers are pushed to `ThinkerAlloc`.
    /// This applies to the map data also where `load()` should be called after
    /// the locations is set in concrete. Other common tasks to do after `new()`
    /// are spawning specials, things.
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
        let map_data = MapData::default();

        // G_DoReborn
        // G_CheckSpot

        Level {
            map_data,
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
            animations: Default::default(),
            button_list: Vec::with_capacity(50),
            line_special_list: Vec::with_capacity(50),
            game_mode,
            snd_command,
            players_in_game,
            players,
            active_platforms: Vec::new(),
            sky_num: 0,
        }
    }

    pub(super) fn stop_platform(&mut self, tag: i16) {
        for plat in self.active_platforms.iter_mut() {
            let plat = unsafe { &mut **plat };
            if plat.tag == tag && plat.status != PlatStatus::InStasis {
                plat.old_status = plat.status;
                plat.status = PlatStatus::InStasis;
            }
        }
    }

    pub(super) fn activate_platform_in_stasis(&mut self, tag: i16) {
        for plat in self.active_platforms.iter_mut() {
            let plat = unsafe { &mut **plat };
            if plat.tag == tag && plat.status == PlatStatus::InStasis {
                plat.status = plat.old_status;
            }
        }
    }

    pub(super) fn add_active_platform(&mut self, platform: *mut Platform) {
        self.active_platforms.push(platform);
    }

    /// # Safety
    /// The platform *must* be live. For example do not call `.mark_remove()`
    /// before `remove_active_platform()`.
    pub(super) unsafe fn remove_active_platform(&mut self, plat: &mut Platform) {
        let mut index = self.active_platforms.len() + 1;
        for (i, p) in self.active_platforms.iter().enumerate() {
            if ptr::eq(*p, plat) {
                index = i;
                break;
            }
        }
        if index < self.active_platforms.len() {
            unsafe { &mut *plat.thinker }.mark_remove();
            self.active_platforms.remove(index);
        }
    }

    pub(super) fn players_in_game(&self) -> &[bool; MAXPLAYERS] {
        unsafe { &*self.players_in_game }
    }

    pub(super) fn players(&self) -> &[Player; MAXPLAYERS] {
        unsafe { &*self.players }
    }

    pub(super) fn players_mut(&mut self) -> &mut [Player; MAXPLAYERS] {
        unsafe { &mut *self.players }
    }

    pub fn load(
        &mut self,
        map_name: &str,
        game_mode: GameMode,
        pic_data: &mut PicData,
        wad_data: &WadData,
    ) {
        let animations = PicAnimation::init(pic_data);
        let switch_list = Switches::init(self.game_mode, pic_data);

        pic_data.set_sky_pic(game_mode, self.options.episode, self.options.map);
        self.sky_num = pic_data.sky_num();

        self.map_data.load(map_name, pic_data, wad_data);
        self.animations = animations;
        self.switch_list = switch_list;
        unsafe {
            self.thinkers = ThinkerAlloc::new(self.map_data.things().len() * 2);
        }
    }

    pub(super) const fn do_exit_level(&mut self) {
        self.secret_exit = false;
        self.game_action = Some(GameAction::CompletedLevel);
    }

    pub(super) const fn do_secret_exit_level(&mut self) {
        self.secret_exit = true;
        self.game_action = Some(GameAction::CompletedLevel);
    }

    pub(super) const fn do_completed(&mut self) {
        self.secret_exit = false;
        self.game_action = Some(GameAction::Victory);
    }

    pub(super) fn start_sound(&self, sfx: SfxName, x: f32, y: f32, uid: usize) {
        self.snd_command
            .send(SoundAction::StartSfx { uid, sfx, x, y })
            .unwrap();
    }
}
